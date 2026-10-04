//! The `docker` target: one Compose file at `.isoloom/docker/compose.yml`.
//!
//! - Each network becomes a Compose network with its subnet (gateway `.1`); `internet:
//!   false` makes it `internal`.
//! - Each machine becomes a service with its fixed address on every network; services find
//!   each other by name (Compose DNS).
//! - Machines with services get a healthcheck (TCP probe), so `depends_on` waits for them
//!   to answer and `docker compose up --wait` means "everything answers".
//! - `init:` scripts run once, in one-shot containers of the machine's image on its networks,
//!   after it answers; machines depending on it wait for them to finish.
//! - Inputs become environment variables read from the shell (`${NAME:-}`), only on the
//!   machines that list them.
//! - Checks run in a `check` profile: `docker compose --profile check run --rm isoloom-check`.
//!
//! Nothing is published on the host: the environment is reached from its own networks.

use serde_yaml_ng::{Mapping, Value};

use super::{GenerateError, GeneratedFile, OUTPUT_DIR, address, common_unsupported, header};
use crate::model::{Machine, Spec, Target};

const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
const ROOT: &str = "../..";
/// The check runner: a small image with `sh`, `curl` and busybox `nc`.
const CHECK_IMAGE: &str = "curlimages/curl:8.11.1";

fn s(v: impl Into<String>) -> Value {
    Value::String(v.into())
}

fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut m = Mapping::new();
    for (k, v) in entries {
        m.insert(s(k), v);
    }
    Value::Mapping(m)
}

fn list(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Sequence(items.into_iter().collect())
}

/// The image a machine runs: the published one, or the name its build is tagged with (so
/// its `init:` jobs can reuse it).
fn image_of(spec: &Spec, name: &str, m: &Machine) -> String {
    let d = m.docker.as_ref().expect("docker target: every machine has docker:");
    d.image.clone().unwrap_or_else(|| format!("isoloom/{}-{}", spec.name, name))
}

/// A TCP probe for each service port that works in most images: busybox/BSD `nc`, else bash.
fn probe(m: &Machine) -> String {
    m.services
        .iter()
        .map(|svc| {
            format!(
                "(nc -z 127.0.0.1 {p} 2>/dev/null || bash -c '</dev/tcp/127.0.0.1/{p}' 2>/dev/null)",
                p = svc.port
            )
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn environment(m: &Machine) -> Option<Value> {
    if m.inputs.is_empty() {
        return None;
    }
    let mut env = Mapping::new();
    for name in &m.inputs {
        env.insert(s(name.as_str()), s(format!("${{{name}:-}}")));
    }
    Some(Value::Mapping(env))
}

fn networks_of(m: &Machine, spec: &Spec, with_address: bool) -> Value {
    let mut nets = Mapping::new();
    for (net, octet) in &m.networks {
        let v = if with_address {
            map([("ipv4_address", s(address(spec, net, *octet).to_string()))])
        } else {
            Value::Null
        };
        nets.insert(s(net.as_str()), v);
    }
    Value::Mapping(nets)
}

/// Init job service names for a machine.
fn init_names(name: &str, m: &Machine) -> Vec<String> {
    let n = m.docker.as_ref().map(|d| d.init.len()).unwrap_or(0);
    (1..=n).map(|i| format!("{name}-init-{i}")).collect()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    common_unsupported(spec, Target::Docker)?;

    let mut services = Mapping::new();
    for (name, m) in &spec.machines {
        // The access machine may have no implementation: the runner supplies it.
        let Some(d) = &m.docker else { continue };
        let mut svc = Mapping::new();
        let image = image_of(spec, name, m);
        if let Some(build) = &d.build {
            svc.insert(s("build"), map([("context", s(format!("{ROOT}/{build}")))]));
        }
        svc.insert(s("image"), s(image.clone()));
        svc.insert(s("hostname"), s(name.as_str()));
        svc.insert(s("networks"), networks_of(m, spec, true));
        if let Some(env) = environment(m) {
            svc.insert(s("environment"), env);
        }
        if !m.services.is_empty() {
            svc.insert(
                s("healthcheck"),
                map([
                    ("test", list([s("CMD-SHELL"), s(probe(m))])),
                    ("interval", s("5s")),
                    ("timeout", s("3s")),
                    ("retries", Value::Number(60.into())),
                    ("start_period", s("10s")),
                ]),
            );
        }
        let mut deps = Mapping::new();
        for dep in &m.depends_on {
            let dm = &spec.machines[dep];
            deps.insert(s(dep.as_str()), map([("condition", s("service_healthy"))]));
            for init in init_names(dep, dm) {
                deps.insert(s(init), map([("condition", s("service_completed_successfully"))]));
            }
        }
        if !deps.is_empty() {
            svc.insert(s("depends_on"), Value::Mapping(deps));
        }
        if let Some(r) = m.resources {
            let mut limits = Mapping::new();
            if let Some(c) = r.cpus {
                limits.insert(s("cpus"), s(c.to_string()));
            }
            if let Some(mb) = r.memory_mb {
                limits.insert(s("memory"), s(format!("{mb}M")));
            }
            if !limits.is_empty() {
                svc.insert(s("deploy"), map([("resources", map([("limits", Value::Mapping(limits))]))]));
            }
        }
        svc.insert(s("restart"), s("unless-stopped"));
        services.insert(s(name.as_str()), Value::Mapping(svc));

        // One-shot init jobs, in order, each after the previous one.
        let mut previous: Option<String> = None;
        for (job, script) in init_names(name, m).into_iter().zip(&d.init) {
            let mut j = Mapping::new();
            j.insert(s("image"), s(image.clone()));
            let mounted = format!("/isoloom/init/{}", file_name(script));
            j.insert(s("entrypoint"), list([s("/bin/sh"), s(mounted.clone())]));
            j.insert(s("volumes"), list([s(format!("{ROOT}/{script}:{mounted}:ro"))]));
            j.insert(s("networks"), networks_of(m, spec, false));
            if let Some(env) = environment(m) {
                j.insert(s("environment"), env);
            }
            let mut deps = Mapping::new();
            deps.insert(s(name.as_str()), map([("condition", s("service_healthy"))]));
            if let Some(p) = &previous {
                deps.insert(s(p.as_str()), map([("condition", s("service_completed_successfully"))]));
            }
            j.insert(s("depends_on"), Value::Mapping(deps));
            j.insert(s("restart"), s("no"));
            services.insert(s(job.as_str()), Value::Mapping(j));
            previous = Some(job);
        }
    }

    // The check runner: every check script, in order, on every network.
    if !spec.checks.is_empty() {
        let mut volumes = Vec::new();
        let mut run = Vec::new();
        for (i, c) in spec.checks.iter().enumerate() {
            let mounted = format!("/isoloom/checks/{:02}-{}", i + 1, file_name(c));
            volumes.push(s(format!("{ROOT}/{c}:{mounted}:ro")));
            run.push(format!("echo '== {c}' && sh {mounted}"));
        }
        let mut nets = Mapping::new();
        for net in spec.networks.keys() {
            nets.insert(s(net.as_str()), Value::Null);
        }
        let mut deps = Mapping::new();
        for (name, m) in &spec.machines {
            if m.docker.is_some() && !m.services.is_empty() {
                deps.insert(s(name.as_str()), map([("condition", s("service_healthy"))]));
            }
            if m.docker.is_some() {
                for init in init_names(name, m) {
                    deps.insert(s(init), map([("condition", s("service_completed_successfully"))]));
                }
            }
        }
        let mut c = Mapping::new();
        c.insert(s("image"), s(CHECK_IMAGE));
        c.insert(s("profiles"), list([s("check")]));
        c.insert(s("entrypoint"), list([s("/bin/sh"), s("-c"), s(run.join(" && "))]));
        c.insert(s("volumes"), Value::Sequence(volumes));
        c.insert(s("networks"), Value::Mapping(nets));
        if !deps.is_empty() {
            c.insert(s("depends_on"), Value::Mapping(deps));
        }
        services.insert(s("isoloom-check"), Value::Mapping(c));
    }

    let mut networks = Mapping::new();
    for (net, n) in &spec.networks {
        let gateway = super::gateway(spec, net);
        let mut v = Mapping::new();
        v.insert(
            s("ipam"),
            map([("config", list([map([("subnet", s(n.cidr.as_str())), ("gateway", s(gateway.to_string()))])]))]),
        );
        if !n.internet {
            v.insert(s("internal"), Value::Bool(true));
        }
        networks.insert(s(net.as_str()), Value::Mapping(v));
    }

    let mut root = Mapping::new();
    root.insert(s("name"), s(spec.name.as_str()));
    root.insert(s("services"), Value::Mapping(services));
    root.insert(s("networks"), Value::Mapping(networks));
    let yaml = serde_yaml_ng::to_string(&Value::Mapping(root)).expect("a compose mapping serializes");

    let usage = "# Start:  docker compose -f .isoloom/docker/compose.yml up -d --wait\n# Checks: docker compose -f .isoloom/docker/compose.yml --profile check run --rm isoloom-check\n# Stop:   docker compose -f .isoloom/docker/compose.yml down -v\n";
    Ok(vec![GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/compose.yml"),
        contents: format!("{}{usage}\n{yaml}", header("#")),
    }])
}
