//! One machine, lowered: its settings copied over, its ports named, its files split by source
//! (configuration as files, run-time values as secret files), and its forms as `docker:` and
//! `vm:` blocks, each built from the recipe that gives it.

use std::net::Ipv4Addr;

use indexmap::IndexMap;
use isoloom_spec::model::{self, DockerImpl, Generated, HttpCheck, MachineFile, MachineSecret, Protocol, SecretMount, VmImpl};
use isoloom_spec::syntax::{self, EnvValue, FileSource, Form, Octets, PortProtocol, PortRef, Publish, Recipe, Step, Tpm};

use super::Scope;
use super::overrides;
use super::trail::Trail;

/// A lowered machine, and the inputs its files read (the secret each becomes → the input).
pub(super) struct LoweredMachine {
    pub machine: model::Machine,
    pub inputs: Vec<(String, String)>,
}

/// The secret an input's file becomes: `input-<name>`, kebab-case (`CTF_FLAG` is
/// `input-ctf-flag`).
pub fn input_secret(input: &str) -> String {
    format!("input-{}", input.to_ascii_lowercase().replace('_', "-"))
}

/// Machine `name`, lowered for `scope`.
pub(super) fn lower(source: &syntax::Spec, name: &str, m: &syntax::Machine, scope: Scope, trail: &mut Trail) -> LoweredMachine {
    let (networks, more_addresses) = addresses(&m.networks);
    let (files, secrets, inputs) = files(name, m, trail);
    trail.machine(name, m);
    let mut out = model::Machine {
        networks,
        more_addresses,
        count: None,
        aliases: m.aliases.clone(),
        os: m.os.map(|o| o.to_string()),
        arch: m.hardware.arch,
        rate_limit: m.rate_limit.clone(),
        privileged: m.privileged,
        nested_virtualization: m.hardware.nested_virtualization,
        gpus: m.hardware.gpus.clone(),
        devices: m.hardware.devices.clone(),
        read_only: m.read_only,
        tmpfs: m.tmpfs.clone(),
        shm_size: m.shm_size.clone(),
        dns: m.dns.clone(),
        environment: environment(m),
        sysctls: m.sysctls.clone(),
        ulimits: m.ulimits.clone(),
        ports: ports(m),
        inputs: m
            .environment
            .values()
            .filter_map(|v| match v {
                EnvValue::Input(i) => Some(i.to_string()),
                _ => None,
            })
            .collect(),
        secrets,
        resources: m.resources,
        command: m.command.as_ref().map(|c| c.run.clone()),
        working_dir: m.command.as_ref().and_then(|c| c.cwd.clone()),
        before: Vec::new(),
        healthcheck: m.healthcheck.as_ref().map(|h| healthcheck(m, h)),
        depends_on: m.depends_on.clone(),
        volumes: m.volumes.clone(),
        files,
        disks: m.disks.clone(),
        access: m.access,
        docker: docker(source, name, m, scope),
        vm: vm(source, name, m),
        external: overrides::external(source, name),
        supplied: false,
    };
    // On Proxmox, a machine run as an LXC container is that container only.
    if out.docker.as_ref().is_some_and(|d| d.lxc) {
        out.vm = None;
    }
    LoweredMachine { machine: out, inputs }
}

/// Each network's first address, and the further ones where a list gives several.
fn addresses(networks: &IndexMap<String, Octets>) -> (IndexMap<String, u8>, IndexMap<String, Vec<u8>>) {
    let mut first = IndexMap::new();
    let mut more = IndexMap::new();
    for (n, o) in networks {
        match o {
            Octets::One(a) => {
                first.insert(n.clone(), *a);
            }
            Octets::Several(list) => {
                if let Some((a, rest)) = list.split_first() {
                    first.insert(n.clone(), *a);
                    if !rest.is_empty() {
                        more.insert(n.clone(), rest.to_vec());
                    }
                }
            }
        }
    }
    (first, more)
}

/// The fixed environment values (an input's goes through `inputs`).
fn environment(m: &syntax::Machine) -> IndexMap<String, String> {
    m.environment
        .iter()
        .filter_map(|(k, v)| match v {
            EnvValue::Text(t) => Some((k.clone(), t.clone())),
            EnvValue::Input(_) | EnvValue::Secret(_) => None,
        })
        .collect()
}

/// The ports, each named after its key.
fn ports(m: &syntax::Machine) -> Vec<model::Port> {
    m.ports
        .iter()
        .filter_map(|(name, p)| {
            let port = p.port?.get();
            let (publish, fixed) = match p.publish {
                None => (None, false),
                Some(Publish::Free) => (Some(port), false),
                Some(Publish::Preferred(h)) => (Some(h.get()), false),
                Some(Publish::Fixed(h)) => (Some(h.get()), true),
            };
            Some(model::Port {
                port,
                name: Some(name.to_string()),
                http: p.protocol.is_http(),
                tls: p.protocol.is_tls(),
                publish,
                fixed,
                dormant: p.dormant,
                protocol: if p.protocol == PortProtocol::Udp { Protocol::Udp } else { Protocol::Tcp },
            })
        })
        .collect()
}

/// The healthcheck, its page's port by number.
fn healthcheck(m: &syntax::Machine, h: &syntax::Healthcheck) -> model::Healthcheck {
    model::Healthcheck {
        test: h.test.clone(),
        http: h.http.as_ref().map(|page| HttpCheck {
            port: match &page.port {
                PortRef::Number(n) => n.get(),
                PortRef::Name(n) => m.ports.get(n).and_then(|p| p.port).map_or(0, std::num::NonZeroU16::get),
            },
            path: page.path.clone(),
        }),
        interval: h.interval.clone(),
        timeout: h.timeout.clone(),
        retries: h.retries,
        start_period: h.start_period.clone(),
    }
}

/// The machine's files split by source: configuration as files, values as secret files (an
/// input's as the secret `input-<name>`), and the inputs read that way.
fn files(name: &str, m: &syntax::Machine, trail: &mut Trail) -> (Vec<MachineFile>, Vec<MachineSecret>, Vec<(String, String)>) {
    let windows = m.os.is_some_and(syntax::OsRef::is_windows);
    let (mut files, mut secrets, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
    for (i, f) in m.files.iter().enumerate() {
        let (source, content) = match &f.source {
            FileSource::Source(p) => (Some(p.clone()), None),
            FileSource::Content(c) => (None, Some(c.clone())),
            FileSource::Input(input) => {
                let secret = input_secret(input.as_str());
                inputs.push((secret.clone(), input.to_string()));
                trail.file(name, "secrets", secrets.len(), i);
                secrets.push(value_file(&secret, f, windows));
                continue;
            }
            FileSource::Secret(secret) => {
                trail.file(name, "secrets", secrets.len(), i);
                secrets.push(value_file(secret, f, windows));
                continue;
            }
        };
        trail.file(name, "files", files.len(), i);
        files.push(MachineFile {
            path: f.path.clone(),
            source,
            content,
            uid: f.uid,
            gid: f.gid,
            mode: f.mode.map(|m| m.to_string()),
        });
    }
    (files, secrets, inputs)
}

/// A value's file: the secret at its path, root's and `0400` unless the spec says (no mode on
/// Windows).
fn value_file(secret: &str, f: &syntax::File, windows: bool) -> MachineSecret {
    MachineSecret::Mount(SecretMount {
        source: secret.to_string(),
        target: Some(f.path.clone()),
        uid: f.uid,
        gid: f.gid,
        mode: (!windows).then(|| f.mode_or_default().bits()),
    })
}

/// The container form, as a `docker:` block.
fn docker(source: &syntax::Spec, name: &str, m: &syntax::Machine, scope: Scope) -> Option<DockerImpl> {
    let idle = m.command.is_none();
    let mut d = DockerImpl {
        image: None,
        build: None,
        dockerfile: None,
        target: None,
        args: IndexMap::new(),
        init: m.setup.clone(),
        idle: false,
        appliance: None,
        config: None,
        firmware: None,
        lxc: overrides::lxc(source, name, scope),
        generated: None,
    };
    match syntax::container_form(m).ok()? {
        Form::Supplied | Form::Service => return None,
        Form::Recipe(Recipe::Image(i)) => d.image = Some(i.clone()),
        Form::Recipe(Recipe::Build(b)) => {
            d.build = Some(b.context.clone());
            d.dockerfile.clone_from(&b.dockerfile);
            d.target.clone_from(&b.target);
            d.args.clone_from(&b.args);
        }
        Form::Recipe(Recipe::Appliance(a)) => {
            d.image.clone_from(&a.image);
            d.appliance = Some(a.kind);
            d.config.clone_from(&a.config);
            d.firmware.clone_from(&a.firmware);
        }
        Form::Steps => {
            let os = m.os?.name();
            d.generated = Some(Generated {
                os: os.to_string(),
                base: isoloom_spec::os::container_image(os)?.to_string(),
                steps: m.app().map(|a| a.steps.clone()).unwrap_or_default(),
            });
            d.idle = idle;
        }
        Form::Os => {
            d.image = Some(isoloom_spec::os::container_image(m.os?.name())?.to_string());
            d.idle = idle;
        }
    }
    Some(d)
}

/// The VM form, as a `vm:` block: a fresh VM of the OS, set up by the steps a VM runs.
fn vm(source: &syntax::Spec, name: &str, m: &syntax::Machine) -> Option<VmImpl> {
    if !matches!(syntax::vm_form(m).ok()?, Form::Os | Form::Steps) {
        return None;
    }
    let provision = m
        .app()
        .map(|a| {
            a.steps
                .iter()
                .filter_map(|s| match s {
                    Step::Run { script, .. } => Some(script.clone()),
                    Step::Reboot => Some(model::REBOOT_STEP.to_string()),
                    Step::Playbook(p) => Some(p.clone()),
                    // Configuration steps on a VM arrive in Phase 3: refused there.
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Some(VmImpl {
        os: m.os?.name().to_string(),
        provision,
        image: overrides::vm_image(source, name),
        tpm: match m.hardware.tpm {
            Tpm::Auto => None,
            Tpm::Required => Some(true),
            Tpm::None => Some(false),
        },
        boot: m.hardware.boot.clone(),
        setup: m.setup.clone(),
    })
}

/// DNS servers named by machine, replaced by that machine's address on a network both share
/// (its Docker block's on the Docker targets).
pub(super) fn resolve_dns(spec: &mut model::Spec, scope: Scope) {
    let laid_out = match scope.target() {
        Some(t) => crate::environment::lay_out(spec, t),
        None => spec.clone(),
    };
    for (name, m) in &mut spec.machines {
        let Some(dns) = &mut m.dns else { continue };
        for server in &mut dns.servers {
            if server.parse::<std::net::IpAddr>().is_ok() {
                continue;
            }
            if let Some(a) = address_on_shared(&laid_out, name, server) {
                *server = a.to_string();
            }
        }
    }
}

/// `server`'s address on the first network it shares with `machine`.
fn address_on_shared(spec: &model::Spec, machine: &str, server: &str) -> Option<Ipv4Addr> {
    let (m, s) = (spec.machines.get(machine)?, spec.machines.get(server)?);
    let net = m.networks.keys().find(|n| s.networks.contains_key(*n) && spec.networks.contains_key(*n))?;
    Some(crate::addressing::address(spec, net, s.networks[net]))
}
