//! A machine's pod: its container, its `init:` jobs' container, the init containers that run
//! before them (Isoloom's probe, the volumes' seeds, the waits for its dependencies) and its
//! volumes.
//!
//! A volume is seeded from the image as Docker seeds a named volume the first time it is
//! mounted: an init container of the machine's image copies what the image has at the path
//! into the claim while it is empty (see [`seed_script`]).

use isoloom_render::constraints;
use isoloom_spec::shell;

use crate::files;
use crate::kustomize::{SCRIPTS, key};
use crate::machine::{Machine, udp};
use crate::network::MACHINE_LABEL;
use crate::objects::{
    Affinity, AffinityTerm, Container, ContainerPort, ContainerSecurity, EmptyDir, EnvVar, Image, Labels, Limits, NameValue, PodAffinity, PodSecurity, PodSpec,
    Resources, SecretKey, SecretSource, Selector, Volume, VolumeMount, VolumeSource,
};
use crate::process::{self, SCRIPTS_DIR};
use crate::storage;

// Isoloom's static busybox (the probe needs nothing from the machine's image) and its utility
// image, as on Compose.
use isoloom_render::container::{PROBE_DIR, PROBE_IMAGE, UTILITY_IMAGE};

/// The `emptyDir` holding Isoloom's probe, and the init container filling it.
pub(crate) const PROBE_VOLUME: &str = "isoloom-probe";
/// Where a seed init container sees the claim it fills.
pub(crate) const SEED_DIR: &str = "/isoloom-seed";
/// In the claim while a seed copy runs (see [`seed_script`]).
pub(crate) const SEED_MARKER: &str = ".isoloom-seeding";
/// The Secret the machines' inputs are read from.
pub(crate) const INPUTS: &str = "isoloom-inputs";
/// A machine's image is only pulled when the node doesn't have it (a built one is only local).
const IF_NOT_PRESENT: Option<&str> = Some("IfNotPresent");

/// The pod of machine `m`.
pub(crate) fn spec(m: &Machine) -> PodSpec {
    let mounts = mounts(m);
    let mut containers = vec![machine_container(m, &mounts)];
    containers.extend(jobs_container(m, &mounts));
    let sysctls: Vec<NameValue> = m
        .plan
        .settings
        .sysctls
        .iter()
        .map(|(k, v)| NameValue {
            name: k.clone(),
            value: v.clone(),
        })
        .collect();
    let (dns_policy, dns_config) = crate::dns::config(m.spec.dns.as_ref());
    PodSpec {
        hostname: Some(m.name.to_owned()),
        subdomain: crate::dns::subdomain(m.spec.dns.as_ref()),
        affinity: beside_owners(m),
        // Namespaced sysctls are the pod's (the kubelet allows the safe ones; others need its
        // --allowed-unsafe-sysctls).
        security_context: (!sysctls.is_empty()).then_some(PodSecurity { sysctls }),
        init_containers: init_containers(m),
        containers,
        // Scheduled on a node of the machine's architecture.
        node_selector: Labels::from([("kubernetes.io/arch".to_owned(), m.plan.arch.id().to_owned())]),
        // A resolver of its own (the lab's domain controller), or the cluster's with the
        // machine's search domains and options.
        dns_policy,
        dns_config,
        volumes: volumes(m),
        ..PodSpec::default()
    }
}

/// The machine's own container.
fn machine_container(m: &Machine, mounts: &[VolumeMount]) -> Container {
    let mut c = Container::new(m.name, m.image());
    c.image_pull_policy = IF_NOT_PRESENT;
    // A stock image the runner supplies for the user to work from, or one the spec marks
    // `idle`: kept running idle.
    if m.spec.supplied || m.spec.docker.as_ref().is_some_and(|d| d.idle) {
        c.command = vec!["sleep".into(), "infinity".into()];
    }
    process::apply(m, &mut c);
    if m.spec.privileged || m.spec.read_only {
        c.security_context = Some(ContainerSecurity {
            privileged: m.spec.privileged,
            read_only_root_filesystem: m.spec.read_only,
            ..ContainerSecurity::default()
        });
    }
    c.ports = m
        .spec
        .ports
        .iter()
        .map(|sv| ContainerPort {
            container_port: sv.port,
            protocol: udp(sv.protocol),
        })
        .collect();
    // Ready when its services answer (dormant ones aren't waited for) and its own healthcheck
    // passes.
    process::probes(m, &mut c);
    c.env = env(m);
    c.resources = limits(m);
    crate::gpus::apply(m.plan.hardware.gpus.as_ref(), &mut c);
    c.volume_mounts = mounts.to_vec();
    c.volume_devices = storage::devices(m);
    c
}

/// Its CPU and memory limits, if it has any.
fn limits(m: &Machine) -> Option<Resources> {
    let limits = Limits {
        cpu: m.plan.size.cpus.map(|c| c.to_string()),
        memory: m.plan.size.memory_mb.map(|mb| format!("{mb}Mi")),
        ..Limits::default()
    };
    (limits.cpu.is_some() || limits.memory.is_some()).then_some(Resources { limits })
}

/// The machine's environment: the spec's constraints (proxy, package mirrors), its own fixed
/// values, then its inputs from the `isoloom-inputs` Secret (optional: unset inputs are empty).
fn env(m: &Machine) -> Vec<EnvVar> {
    let mut fixed = constraints::machine_env(m.env.spec());
    fixed.extend(m.plan.settings.environment.iter().map(|(k, v)| (k.clone(), v.clone())));
    let inputs = m.plan.inputs.iter().map(|i| EnvVar::Secret {
        name: i.clone(),
        value_from: SecretSource {
            secret_key_ref: SecretKey {
                name: INPUTS.into(),
                key: i.clone(),
                optional: true,
            },
        },
    });
    fixed
        .into_iter()
        .map(|(name, value)| EnvVar::Value(NameValue { name, value }))
        .chain(inputs)
        .collect()
}

/// Where the machine sees its volumes: its claims, its memory-backed mounts (tmpfs, a sized
/// /dev/shm) and Isoloom's probe (read-only), which the readiness probe runs.
fn mounts(m: &Machine) -> Vec<VolumeMount> {
    let mut out = storage::mounts(m);
    out.extend(storage::tmpfs(m).0);
    if m.spec.shm_size.is_some() {
        out.push(VolumeMount::at("dshm", "/dev/shm"));
    }
    if m.probed() {
        out.push(VolumeMount::read_only(PROBE_VOLUME, PROBE_DIR));
    }
    out.extend(crate::secrets::mounts(m));
    out.extend(files::mounts(m));
    out
}

/// Its `init:` jobs, in order, once it answers: a second container of the same pod (its
/// network, its volumes), kept running idle after them.
fn jobs_container(m: &Machine, mounts: &[VolumeMount]) -> Option<Container> {
    if m.init().is_empty() {
        return None;
    }
    let wait = m.probed().then(|| format!("until {}; do sleep 2; done", process::ready_test(m)));
    let run = m.init().iter().map(|script| format!("sh {SCRIPTS_DIR}/{}", key(script)));
    let steps: Vec<String> = wait.into_iter().chain(run).chain(["exec sleep infinity".to_owned()]).collect();
    let mut c = Container::new("init", m.image());
    c.image_pull_policy = IF_NOT_PRESENT;
    c.command = vec!["sh".into(), "-c".into(), steps.join(" && ")];
    c.env = env(m);
    c.volume_mounts = mounts.to_vec();
    c.volume_mounts.push(VolumeMount::at("scripts", SCRIPTS_DIR));
    Some(c)
}

/// Whether the pod needs Isoloom's busybox: for the readiness probe and for seeding volumes.
fn needs_probe(m: &Machine) -> bool {
    m.probed() || !m.plan.volumes.is_empty()
}

/// What runs before the machine, in order: Isoloom's busybox copied in, the volumes it holds
/// seeded (a shared volume once, by its owner), then the waits for its dependencies' services.
fn init_containers(m: &Machine) -> Vec<Container> {
    let mut out = Vec::new();
    if needs_probe(m) {
        let mut c = Container::new(PROBE_VOLUME, Image::Tool(m.env.images.pull(PROBE_IMAGE)));
        c.command = vec!["/bin/cp".into(), "/bin/busybox".into(), "/probe/busybox".into()];
        c.volume_mounts = vec![VolumeMount::at(PROBE_VOLUME, "/probe")];
        out.push(c);
    }
    out.extend(files::init_container(m, "scripts"));
    out.extend(crate::secrets::init_container(m));
    let held = m.plan.volumes.iter().filter(|v| m.holds(&v.kind));
    out.extend(held.map(|v| seed_container(m, &v.name, &v.path)));
    out.extend(m.plan.depends_on.iter().filter_map(|dep| wait_for(m, dep)));
    out
}

/// Waits until dependency `dep`'s services answer, if it has any to probe.
fn wait_for(m: &Machine, dep: &str) -> Option<Container> {
    let ports = m.env.spec().machines.get(dep)?.ready_ports();
    if ports.is_empty() {
        return None;
    }
    let cond = ports.iter().map(|p| format!("nc -z {dep} {p}")).collect::<Vec<_>>().join(" && ");
    let mut c = Container::new(format!("wait-{dep}"), Image::Tool(m.env.images.pull(UTILITY_IMAGE)));
    c.command = vec!["sh".into(), "-c".into(), format!("until {cond}; do sleep 2; done")];
    Some(c)
}

/// An init container that gives volume `v` (mounted at `path`) the image's files at that path
/// while the claim is empty (see [`seed_script`]). It runs the machine's image (its files) with
/// Isoloom's static busybox (no shell or `cp` needed from the image: distroless and `scratch`
/// images are seeded too), as root so `cp -a` keeps owners and modes.
fn seed_container(m: &Machine, v: &str, path: &str) -> Container {
    let mut c = Container::new(format!("seed-{v}"), m.image());
    c.image_pull_policy = IF_NOT_PRESENT;
    c.command = vec![format!("{PROBE_DIR}/busybox"), "sh".into(), "-c".into(), seed_script(path)];
    c.security_context = Some(ContainerSecurity {
        run_as_user: Some(0),
        run_as_group: Some(0),
        ..ContainerSecurity::default()
    });
    c.volume_mounts = vec![VolumeMount::at(format!("vol-{v}"), SEED_DIR), VolumeMount::read_only(PROBE_VOLUME, PROBE_DIR)];
    c
}

/// The seed's copy, as Docker does for a named volume the first time it is mounted: a machine
/// declaring `volumes: { ctf: /chroot }` keeps the image's `/chroot` and its init jobs add to
/// it. Empty means nothing in the claim but a `lost+found` (ext4 claims have one); a path the
/// image doesn't have leaves the claim empty. Once seeded, or written by the machine, a claim
/// is never seeded again (its data survives restarts).
///
/// A `.isoloom-seeding` marker stands in the claim while the copy runs: a copy that failed (the
/// claim too small for the image's files) fails the pod, and the next try finds the marker,
/// clears what the last one left and copies again, instead of taking a half-filled claim as
/// seeded.
pub(crate) fn seed_script(path: &str) -> String {
    let src = shell::quote(&format!("{}/.", path.trim_end_matches('/')));
    format!(
        "d={SEED_DIR}; m=\"$d/{SEED_MARKER}\"; b={PROBE_DIR}/busybox; [ -d {path} ] || exit 0; for f in \"$d\"/* \"$d\"/.[!.]* \"$d\"/..?*; do if [ -e \"$f\" ] || [ -L \"$f\" ]; then case \"$f\" in \"$d/lost+found\" | \"$m\") ;; *) [ -e \"$m\" ] || exit 0; $b rm -rf \"$f\" ;; esac; fi; done; : > \"$m\" && $b cp -a {src} \"$d/\" && $b rm -f \"$m\"",
        path = shell::quote(path),
    )
}

/// A machine mounting shared volumes it doesn't own runs on their owners' node: one claim,
/// mounted by every pod of that node (`ReadWriteOnce` allows it), on any storage class.
fn beside_owners(m: &Machine) -> Option<Affinity> {
    let terms: Vec<AffinityTerm> = m
        .plan
        .volumes
        .iter()
        .filter_map(|v| match &v.kind {
            isoloom_plan::VolumeKind::Shared { owner, .. } if owner != m.name => Some(AffinityTerm {
                label_selector: Selector::of(MACHINE_LABEL, owner),
                topology_key: "kubernetes.io/hostname",
            }),
            _ => None,
        })
        .collect();
    (!terms.is_empty()).then_some(Affinity {
        pod_affinity: PodAffinity { required: terms },
    })
}

/// The pod's volumes: its claims, the project's scripts (for its `setup` jobs and its files),
/// Isoloom's probe, then its memory-backed mounts.
fn volumes(m: &Machine) -> Vec<Volume> {
    let volume = |name: String, source| Volume { name, source };
    let mut out = storage::pod_volumes(m);
    if !m.init().is_empty() || files::reads_project(m) {
        out.push(volume("scripts".into(), scripts_volume()));
    }
    out.extend(files::volumes(m));
    if needs_probe(m) {
        out.push(volume(PROBE_VOLUME.into(), VolumeSource::EmptyDir(EmptyDir::default())));
    }
    let memory = |size: Option<&String>| {
        VolumeSource::EmptyDir(EmptyDir {
            medium: Some("Memory"),
            size_limit: size.cloned(),
        })
    };
    out.extend(storage::tmpfs(m).1);
    if let Some(shm) = &m.spec.shm_size {
        out.push(volume("dshm".into(), memory(Some(shm))));
    }
    out.extend(crate::secrets::volumes(m));
    out
}

/// The project's scripts, executable.
pub(crate) fn scripts_volume() -> VolumeSource {
    config_map_volume(SCRIPTS)
}

/// A `ConfigMap`'s files, executable.
pub(crate) fn config_map_volume(name: &str) -> VolumeSource {
    VolumeSource::ConfigMap {
        name: name.into(),
        default_mode: 0o755,
    }
}
