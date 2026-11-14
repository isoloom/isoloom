//! A machine's Compose service: its image or build, runtime settings, networks, readiness check,
//! then its sidecar; its appliance wiring ([`appliance`]) and `init:` jobs ([`init_jobs`]) are
//! their own modules.

use indexmap::IndexMap;
use isoloom_manifest as manifest;
use isoloom_plan::{Environment, Image, PublishedPort};
use isoloom_render::configure::dockerfile;
use isoloom_render::constraints;
use isoloom_render::container::{PROBE_DIR, init_names};
use isoloom_spec::contract::{PUBLISH_ADDRESS, PUBLISH_FIXED};
use isoloom_spec::model;

use super::{Out, appliance, configs, hardware, init_jobs, process, secrets};
use crate::file::{Attachment, Build, Condition, Deploy, Limits, Resources, Restart, Service, Ulimit};
use crate::interpolation::literal;
use crate::layout::{ROOT, relative_to};
use crate::machine::Machine;
use crate::names::{probe_service, routes_service};
use crate::trunks;
use crate::{labels, networks, outside, sidecars, storage};

/// A machine's service, then its sidecar and its `init:` jobs.
pub(super) fn machine(env: &Environment, m: Machine<'_>, ts: &[trunks::Trunk], out: &mut Out) {
    let mut svc = base(env, m, ts);
    appliance::wire(env, m, &mut svc, &mut out.files);
    waits(env, m, &mut svc, &mut out.probes);
    svc.deploy = limits(m.plan);
    svc.labels.extend(labels::ports(m.spec));
    hardware::machine(&m.plan.hardware, &mut svc);
    out.services.insert(m.name.to_owned(), svc);
    if sidecars::needed(env, m) {
        let mut routes = sidecars::routes(env, m.name, m);
        // The access box's route keeper follows it into the `access` profile: a non-profiled
        // service that depends on a profiled one would drag it into the initial `up`.
        if m.plan.is_access() {
            routes.profiles = vec![manifest::access::PROFILE.to_owned()];
        }
        out.services.insert(routes_service(m.name), routes);
    }
    init_jobs::add(env, m, &mut out.services);
}

/// What a machine that waits for `m` waits for: healthy (its probe passes) when it has a port
/// to probe or a healthcheck of its own, else started.
pub(crate) fn readiness(m: &model::Machine) -> Condition {
    if m.has_readiness() { Condition::Healthy } else { Condition::Started }
}

/// What every machine's service has: its image, runtime settings, names and networks.
fn base(env: &Environment, m: Machine<'_>, ts: &[trunks::Trunk]) -> Service {
    let (spec, plan) = (m.spec, m.plan);
    let (tmpfs, long_tmpfs) = storage::tmpfs(spec);
    let mut svc = Service {
        platform: platform(plan),
        privileged: spec.privileged,
        read_only: spec.read_only,
        sysctls: plan.settings.sysctls.clone(),
        ulimits: plan
            .settings
            .ulimits
            .iter()
            .map(|(k, u)| (k.clone(), Ulimit::new(u.soft(), u.hard())))
            .collect(),
        shm_size: spec.shm_size.clone(),
        tmpfs,
        hostname: Some(m.name.to_owned()),
        networks: networks::attachments(env, plan),
        extra_hosts: networks::extra_hosts(env, m),
        environment: environment(env, plan),
        ports: plan.ports.iter().map(port).collect(),
        volumes: [storage::volume_mounts(plan), long_tmpfs, storage::host_folders(m.name, plan)].concat(),
        secrets: secrets::of_machine(plan),
        configs: configs::mounts(m.name, plan),
        restart: Some(Restart::UnlessStopped),
        ..Service::default()
    };
    if let Some(Image::Container { reference, build, .. }) = &plan.image {
        svc.image = Some(reference.clone());
        svc.build = build.as_ref().map(|b| build_of(env, m.name, b));
    }
    dns(spec, &mut svc);
    // A stock image the runner supplies for the user to work from, or one the spec marks
    // `idle`: kept running idle (its own command may be a shell that exits at once).
    if spec.supplied || spec.docker.as_ref().is_some_and(|d| d.idle) {
        svc.init = true;
        svc.entrypoint = vec!["sleep".into(), "infinity".into()];
    }
    // The access machine: brought up on demand, never by the initial `up`. It sits in the
    // `access` Compose profile (so `up` skips it) and runs a parameterizable image the runner can
    // override when it hot-adds it; it stays up idle for the user to work from.
    if plan.is_access() {
        access_box(plan, &mut svc);
    }
    // Isoloom's supervisor runs its services: under the engine's init, which reaps what they leave.
    if let Some(Image::Container {
        build: Some(isoloom_plan::Build::Generated { process, .. }),
        ..
    }) = &plan.image
    {
        svc.init |= process.is_supervisor();
    }
    process::apply(plan, &mut svc);
    // A VLAN it reaches through a trunk isn't an interface of its own: the trunk is.
    svc.networks.retain(|k, _| !trunks::carried(ts, m.name, k));
    for t in trunks::of(ts, m.name) {
        svc.networks
            .insert(trunks::link_network(t), Some(Attachment::with_mac(trunks::mac(t, trunks::End::Machine))));
    }
    svc.networks.extend(outside::attachments(plan));
    if plan.is_gateway {
        // It routes: forwarding on (beside the spec's own kernel parameters), and the right to
        // set its own firewall rules.
        svc.cap_add = vec!["NET_ADMIN".into()];
        svc.sysctls.extend(crate::ipv6::forwarding(env));
    }
    svc
}

/// The platform a machine's containers (its own, its `init:` jobs) are pinned to, when it is
/// pinned to one: else Docker runs them on the host's architecture.
pub(super) fn platform(plan: &isoloom_plan::MachinePlan) -> Option<String> {
    plan.platform.map(|a| a.docker_platform().to_owned())
}

/// The machine's DNS, when the spec sets it: its servers, options, search list and domain.
fn dns(spec: &model::Machine, svc: &mut Service) {
    if let Some(dns) = &spec.dns {
        svc.dns.clone_from(&dns.servers);
        svc.dns_opt.clone_from(&dns.options);
        svc.dns_search.clone_from(&dns.search);
        svc.domainname.clone_from(&dns.domain);
    }
}

/// The access machine: brought up on demand, never by the initial `up`. It sits in the `access`
/// Compose profile (so `up` skips it) and stays up idle for the user to work from. An environment that
/// builds its own image for it (a `build`) keeps it; otherwise it runs its planned image, which
/// the runner can override with one it is given.
fn access_box(plan: &isoloom_plan::MachinePlan, svc: &mut Service) {
    svc.profiles = vec![manifest::access::PROFILE.to_owned()];
    svc.init = true;
    svc.entrypoint = vec!["sleep".into(), "infinity".into()];
    if let (None, Some(Image::Container { reference, .. })) = (&svc.build, &plan.image) {
        svc.image = Some(format!("${{{}:-{reference}}}", manifest::contract::ACCESS_IMAGE));
    }
}

/// A published port: on the host's loopback, on an ephemeral host port by default (no
/// collisions); `ISOLOOM_PUBLISH_FIXED` pins it to `publish` inside the cloud VMs that forward
/// it. `<port>/udp` for a UDP service (Compose's default is TCP).
fn port(p: &PublishedPort) -> String {
    let proto = if p.protocol.is_tcp() { "" } else { "/udp" };
    let (publish, port) = (p.publish, p.port);
    if p.fixed {
        format!("${{{PUBLISH_ADDRESS}:-127.0.0.1}}:{publish}:{port}{proto}")
    } else {
        format!("${{{PUBLISH_ADDRESS}:-127.0.0.1}}:${{{PUBLISH_FIXED}:+{publish}}}:{port}{proto}")
    }
}

/// A machine's Compose `build`: its context, and the Dockerfile (Compose reads it relative to
/// the context) and build arguments when the spec gives them; the spec's own arguments win over
/// the constraints'. `$$`: a literal value, not Compose's interpolation. A generated Dockerfile
/// builds with the project folder as its context.
pub(super) fn build_of(env: &Environment, machine: &str, b: &isoloom_plan::Build) -> Build {
    let constraints = constraints::machine_env(env.constraints.as_ref());
    let (context, dockerfile, target, own) = match b {
        isoloom_plan::Build::Own {
            context,
            dockerfile,
            target,
            args,
        } => (
            format!("{ROOT}/{context}"),
            dockerfile.as_ref().map(|f| relative_to(context, f)),
            target.clone(),
            args.clone(),
        ),
        isoloom_plan::Build::Generated { .. } => (ROOT.to_owned(), Some(dockerfile::path(machine)), None, IndexMap::new()),
    };
    Build {
        context,
        dockerfile,
        target,
        args: constraints
            .into_iter()
            .filter(|(k, _)| !own.contains_key(k))
            .chain(own.clone())
            .map(|(k, v)| (k, literal(&v)))
            .collect(),
        labels: IndexMap::new(),
    }
}

/// A machine's environment: the spec's constraints (proxy, package mirrors), its own fixed
/// values (`$$`: literal, not Compose's interpolation), then its inputs, read from the shell.
pub(super) fn environment(env: &Environment, plan: &isoloom_plan::MachinePlan) -> Option<IndexMap<String, String>> {
    let constraints = constraints::machine_env(env.constraints.as_ref());
    if plan.inputs.is_empty() && constraints.is_empty() && plan.settings.environment.is_empty() {
        return None;
    }
    let fixed = plan.settings.environment.iter().map(|(k, v)| (k.clone(), v.clone()));
    let mut out: IndexMap<String, String> = constraints.into_iter().chain(fixed).map(|(k, v)| (k, literal(&v))).collect();
    out.extend(plan.inputs.iter().map(|i| (i.clone(), format!("${{{i}:-}}"))));
    Some(out)
}

/// The healthcheck (the port probe and the machine's own) and what the machine waits for before
/// it starts.
fn waits(env: &Environment, m: Machine<'_>, svc: &mut Service, probes: &mut super::Probes) {
    let plan = m.plan;
    if let Some(healthcheck) = process::healthcheck(m.spec, plan) {
        svc.healthcheck = Some(healthcheck);
        // Isoloom's own probe, built for the machine's architecture, copied into a volume
        // before the machine starts.
        let probe_svc = probe_service(plan.platform);
        svc.volumes.push(format!("{probe_svc}:{PROBE_DIR}:ro").into());
        svc.after(probe_svc, Condition::CompletedSuccessfully);
        match probes.iter_mut().find(|(p, _)| *p == plan.platform) {
            Some((_, machines)) => machines.push(m.name.to_owned()),
            None => probes.push((plan.platform, vec![m.name.to_owned()])),
        }
    }
    for dep in plan.depends_on.iter().filter_map(|d| Machine::of(env, d)) {
        svc.after(dep.name, readiness(dep.spec));
        for init in init_names(dep.name, dep.spec) {
            svc.after(init, Condition::CompletedSuccessfully);
        }
        if sidecars::needed(env, dep) {
            svc.after(routes_service(dep.name), Condition::Healthy);
        }
    }
}

/// The machine's CPU and memory limits, when it has some.
fn limits(plan: &isoloom_plan::MachinePlan) -> Option<Deploy> {
    let limits = Limits {
        cpus: plan.resources.cpus.map(|c| c.to_string()),
        memory: plan.resources.memory_mb.map(|mb| format!("{mb}M")),
    };
    (limits.cpus.is_some() || limits.memory.is_some()).then_some(Deploy {
        resources: Resources {
            limits: Some(limits),
            reservations: None,
        },
    })
}

#[cfg(test)]
mod tests {
    /// The Compose file of the spec `machines` (YAML under `machines:`), on one network.
    fn file_of(machines: &str) -> crate::file::ComposeFile {
        let env = crate::testing::environment(&format!("version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\n{machines}"));
        crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file
    }

    fn yaml<T: serde::Serialize>(value: &T) -> String {
        serde_yaml_ng::to_string(value).unwrap()
    }

    #[test]
    fn an_init_job_of_a_machine_that_builds_its_image_builds_it_too_so_no_pull_asks_a_registry_for_it() {
        let file = file_of(
            "machines:\n  database:\n    networks: { lan: 10 }\n    ports: { pg: 5432 }\n    setup: [db/seed.sh]\n    app: { build: { context: db, args: { V: '1' } } }\n  web: { networks: { lan: 20 }, setup: [seed.sh], app: { image: nginx } }\n",
        );
        let (db, job) = (&file.services["database"], &file.services["isoloom-init-database-1"]);
        assert_eq!(job.image, db.image, "the image the machine builds");
        assert!(db.build.is_some());
        // `pull --ignore-buildable` skips a service with a `build`; `pull_policy` wouldn't do, as
        // `pull --policy` overrides it.
        assert_eq!(yaml(&job.build), yaml(&db.build), "the machine's own build");
        let web = &file.services["isoloom-init-web-1"];
        assert!(web.build.is_none() && web.image.as_deref() == Some("nginx"), "a pulled image is pulled");
    }

    #[test]
    fn a_container_runs_on_the_hosts_architecture_unless_its_spec_asks_for_one() {
        let file = file_of(
            "machines:\n  web: { networks: { lan: 10 }, ports: { http: 80 }, setup: [seed.sh], app: { image: nginx } }\n  db: { networks: { lan: 20 }, ports: { pg: 5432 }, app: { build: { context: db } } }\n",
        );
        for service in ["web", "isoloom-init-web-1", "db", "isoloom-probe"] {
            assert_eq!(file.services[service].platform, None, "{service}");
        }
        assert_eq!(file.services["isoloom-probe"].entrypoint, ["/bin/cp", "/bin/busybox", "/probe/busybox"]);
        assert!(!file.services.keys().any(|s| s.starts_with("isoloom-probe-")), "{:?}", file.services.keys());
        assert!(yaml(&file.services["web"].volumes).contains("isoloom-probe:/.isoloom-probe:ro"));
    }

    #[test]
    fn a_container_pinned_by_its_spec_and_its_probe_run_on_that_architecture_and_say_how_a_host_that_cant_could() {
        let file = file_of(
            "common: { hardware: { arch: amd64 } }\nmachines:\n  web: { networks: { lan: 10 }, ports: { http: 80 }, setup: [seed.sh], app: { image: nginx } }\n  api: { networks: { lan: 20 }, ports: { http: 80 }, hardware: { arch: arm64 }, app: { image: nginx } }\n",
        );
        let platform = |s: &str| file.services[s].platform.as_deref();
        assert_eq!((platform("web"), platform("isoloom-init-web-1")), (Some("linux/amd64"), Some("linux/amd64")));
        assert_eq!(platform("api"), Some("linux/arm64"));
        let probe = &file.services["isoloom-probe-amd64"];
        assert_eq!(probe.platform.as_deref(), Some("linux/amd64"));
        assert_eq!(probe.entrypoint[..3], ["/isoloom-host-probe/busybox", "sh", "-c"]);
        let script = &probe.entrypoint[3];
        for wanted in [
            "if /bin/busybox true 2>/dev/null; then exec /bin/busybox cp /bin/busybox /probe/busybox; fi",
            "can't run linux/amd64 programs, which machine web needs",
            "$$(/isoloom-host-probe/busybox uname -m)",
            "tonistiigi/binfmt --install amd64",
            "exit 1",
        ] {
            assert!(script.contains(wanted), "{wanted}: {script}");
        }
        assert!(yaml(probe).contains("isoloom-probe:/isoloom-host-probe:ro"), "{}", yaml(probe));
        assert!(
            yaml(&probe.depends_on).contains("isoloom-probe"),
            "after the host's probe: {}",
            yaml(&probe.depends_on)
        );
        // The host's own probe is there for the pinned one to run, unpinned.
        assert_eq!(file.services["isoloom-probe"].platform, None);
        assert!(file.services["isoloom-probe-arm64"].entrypoint[3].contains("which machine api needs"));
        assert!(yaml(&file.services["web"].volumes).contains("isoloom-probe-amd64:/.isoloom-probe:ro"));
        let volumes: Vec<&String> = file.volumes.keys().collect();
        assert_eq!(volumes, ["isoloom-probe", "isoloom-probe-amd64", "isoloom-probe-arm64"]);
    }

    #[test]
    fn a_gateway_keeps_its_own_kernel_parameters_beside_forwarding() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks:\n  dmz: { cidr: 10.9.0.0/24, gateway: fw }\n  out: { cidr: 10.8.0.0/24 }\nmachines:\n  fw:\n    networks: { out: 5, dmz: 1 }\n    sysctls: { net.ipv4.tcp_syncookies: \"1\" }\n    app: { image: alpine }\n",
        );
        let file = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file;
        let fw = &file.services["fw"];
        assert_eq!(fw.sysctls.get("net.ipv4.tcp_syncookies").map(String::as_str), Some("1"), "{:?}", fw.sysctls);
        assert_eq!(fw.sysctls.get("net.ipv4.ip_forward").map(String::as_str), Some("1"), "{:?}", fw.sysctls);
    }

    #[test]
    fn no_machine_is_named_like_another_machines_sidecar_or_job() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { a: { cidr: 10.1.0.0/24, gateway: fw }, b: { cidr: 10.2.0.0/24 } }\nmachines:\n  fw: { networks: { b: 5, a: 1 }, app: { image: alpine } }\n  web: { networks: { a: 10 }, setup: [seed.sh], app: { image: nginx } }\n  web-routes: { networks: { b: 10 }, app: { image: nginx } }\n  web-init-1: { networks: { b: 11 }, app: { image: nginx } }\n",
        );
        let file = crate::render::compose_file(crate::backend::ComposeTarget::Docker, &env).unwrap().file;
        for service in ["web", "web-routes", "web-init-1", "isoloom-routes-web", "isoloom-init-web-1"] {
            assert!(file.services.contains_key(service), "{service}: {:?}", file.services.keys());
        }
        assert_eq!(file.services["web-routes"].image.as_deref(), Some("nginx"));
        assert_eq!(file.services["web-init-1"].image.as_deref(), Some("nginx"));
    }
}
