//! The `kubernetes` target: the environment's containers on a Kubernetes cluster, in
//! `.isoloom/kubernetes/` (plain manifests, applied through kustomize so the project's scripts
//! travel as a ConfigMap).
//!
//! - A namespace per environment; each machine a Deployment (one replica) with its hostname,
//!   and a Service named after it when it serves, so names resolve as on every other target.
//! - Networks and `reach`: NetworkPolicies (deny by default; a network's machines reach each
//!   other; `reach` rules open one network to another, on their ports). No router: the cluster's
//!   network policy engine enforces them (k3s, Calico, Cilium; not every CNI does).
//! - Offline machines: an egress policy keeps them inside the environment (and DNS).
//! - `publish`: a LoadBalancer Service on the published port (local clusters such as OrbStack,
//!   Docker Desktop and k3s answer on localhost).
//! - Volumes: a PersistentVolumeClaim each, seeded from the image as Docker seeds a named
//!   volume: an init container of the machine's image copies what the image has at the path
//!   into the claim while it is empty (see [`seed_container`]). `init:` jobs: a container in
//!   the machine's pod (same network and volumes), run once the machine answers.
//!   `depends_on`: init containers that wait for the dependencies' services.
//! - Checks: a Job per position in `.isoloom/kubernetes/checks/`, with the networks (labels) of
//!   the machine it stands for, so the same policies apply to it.
//! - Addresses: Kubernetes picks pod addresses; names, ports and reachability are kept.
//! - A Helm chart of the same manifests in `.isoloom/kubernetes/chart/` (see [`chart`]): the
//!   machines' images and the inputs as values, the checks as `helm test` Pods, the project's
//!   scripts under `files/project/` (copied there by the command line, see
//!   [`crate::project_copies`]).

use std::fmt::Write;

use serde_yaml_ng::{Mapping, Value};

use super::docker::{CHECK_IMAGE, PROBE_DIR, PROBE_IMAGE, UTILITY_IMAGE, image_of, list, map, offline, probe, probe_command, s};
use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::checks;
use crate::model::{Machine, Protocol, Spec, Target};
use crate::shell;

const DIR: &str = "kubernetes";
/// The project's root, from `.isoloom/kubernetes/`.
const ROOT: &str = "../..";
const SCRIPTS: &str = "isoloom-scripts";
const INPUTS: &str = "isoloom-inputs";

fn namespace(spec: &Spec) -> String {
    format!("isoloom-{}", spec.name)
}

fn net_label(net: &str) -> String {
    format!("net.isoloom.com/{net}")
}

const MACHINE_LABEL: &str = "isoloom.com/machine";
const CHECK_LABEL: &str = "isoloom.com/check";
/// The ConfigMap holding the check runners' scripts.
const CHECK_SCRIPTS: &str = "isoloom-check-runners";

/// An egress policy for the pods `selector` matches: other pods of the environment, and DNS.
fn offline_policy(name: &str, selector: Value) -> Value {
    doc(
        "NetworkPolicy",
        "networking.k8s.io/v1",
        name,
        vec![(
            "spec",
            map([
                ("podSelector", map([("matchLabels", selector)])),
                ("policyTypes", list([s("Egress")])),
                (
                    "egress",
                    list([
                        map([("to", list([map([("podSelector", Value::Mapping(Mapping::new()))])]))]),
                        map([
                            (
                                "to",
                                list([map([(
                                    "namespaceSelector",
                                    map([("matchLabels", labels(&[("kubernetes.io/metadata.name".into(), "kube-system")]))]),
                                )])]),
                            ),
                            (
                                "ports",
                                list([
                                    map([("port", Value::from(53)), ("protocol", s("UDP"))]),
                                    map([("port", Value::from(53)), ("protocol", s("TCP"))]),
                                ]),
                            ),
                        ]),
                    ]),
                ),
            ]),
        )],
    )
}

/// A ConfigMap key for a project file (keys allow letters, digits, `-`, `_` and `.`).
fn key(path: &str) -> String {
    path.replace('/', "__")
}

fn doc(kind: &str, api: &str, name: &str, rest: Vec<(&'static str, Value)>) -> Value {
    let mut m = Mapping::new();
    m.insert(s("apiVersion"), s(api));
    m.insert(s("kind"), s(kind));
    m.insert(s("metadata"), map([("name", s(name))]));
    for (k, v) in rest {
        m.insert(s(k), v);
    }
    Value::Mapping(m)
}

fn labels(pairs: &[(String, &str)]) -> Value {
    let mut m = Mapping::new();
    for (k, v) in pairs {
        m.insert(s(k.as_str()), s(*v));
    }
    Value::Mapping(m)
}

/// The labels a pod on these networks carries (what the network policies select).
fn pod_labels(machine: Option<&str>, nets: &[&String]) -> Value {
    let mut pairs: Vec<(String, &str)> = Vec::new();
    if let Some(m) = machine {
        pairs.push((MACHINE_LABEL.to_string(), m));
    }
    for n in nets {
        pairs.push((net_label(n), "member"));
    }
    labels(&pairs)
}

/// The machine's environment: the spec's constraints (proxy, package mirrors), then its inputs
/// from the `isoloom-inputs` Secret (optional: unset inputs are empty).
fn env(spec: &Spec, m: &Machine) -> Option<Value> {
    let mut constraints = super::constraints::machine_env(spec);
    constraints.extend(m.environment.iter().map(|(k, v)| (k.clone(), v.clone())));
    if m.inputs.is_empty() && constraints.is_empty() {
        return None;
    }
    let fixed = constraints.into_iter().map(|(k, v)| map([("name", s(k)), ("value", s(v))]));
    Some(list(fixed.chain(m.inputs.iter().map(|i| {
        map([
            ("name", s(i.as_str())),
            (
                "valueFrom",
                map([(
                    "secretKeyRef",
                    map([("name", s(INPUTS)), ("key", s(i.as_str())), ("optional", Value::Bool(true))]),
                )]),
            ),
        ])
    }))))
}

fn unsupported(spec: &Spec) -> Option<String> {
    if !spec.provision.is_empty() {
        return Some("environment-level provisioning (`provision:`) runs on VM targets for now".into());
    }
    if let Some(what) = super::container_checks_unsupported(spec) {
        return Some(what);
    }
    if spec.machines.values().any(|m| !m.aliases.is_empty()) {
        return Some("machine `aliases` (dotted DNS names) aren't Kubernetes Service names".into());
    }
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine on Kubernetes come later".into());
    }
    if let Some((name, _)) = spec.machines.iter().find(|(_, m)| !m.ulimits.is_empty()) {
        return Some(format!("machine `{name}` sets `ulimits`: Kubernetes has no per-container process limits"));
    }
    if spec.networks.values().any(|n| n.tc.is_some()) {
        return Some("link impairment (`tc`) needs a router in the path; Kubernetes has none".into());
    }
    None
}

/// Whether machine `name` holds its volume `v`'s claim: its own volume, or a shared one it owns.
fn holds(spec: &Spec, name: &str, v: &str) -> bool {
    !spec.volumes.contains_key(v) || spec.volume_owner(v) == Some(name)
}

/// A machine mounting shared volumes it doesn't own runs on their owners' node: one claim,
/// mounted by every pod of that node (ReadWriteOnce allows it), on any storage class.
fn beside_owners(spec: &Spec, name: &str, m: &Machine) -> Option<Value> {
    let owners: Vec<&str> = m
        .volumes
        .keys()
        .filter(|v| !holds(spec, name, v))
        .filter_map(|v| spec.volume_owner(v))
        .collect();
    if owners.is_empty() {
        return None;
    }
    let terms = owners.iter().map(|o| {
        map([
            ("labelSelector", map([("matchLabels", labels(&[(MACHINE_LABEL.to_string(), *o)]))])),
            ("topologyKey", s("kubernetes.io/hostname")),
        ])
    });
    Some(map([("podAffinity", map([("requiredDuringSchedulingIgnoredDuringExecution", list(terms))]))]))
}

/// The emptyDir holding Isoloom's probe, and the init container filling it.
const PROBE_VOLUME: &str = "isoloom-probe";
/// Where a seed init container sees the claim it fills.
const SEED_DIR: &str = "/isoloom-seed";
/// In the claim while a seed copy runs (see [`seed_container`]).
const SEED_MARKER: &str = ".isoloom-seeding";

/// An init container that gives volume `v` (mounted at `path`) the image's files at that path
/// while the claim is empty, as Docker does for a named volume the first time it is mounted: a
/// machine declaring `volumes: { ctf: /chroot }` keeps the image's `/chroot` and its init jobs
/// add to it. It runs the machine's image (its files) with Isoloom's static busybox (no shell
/// or `cp` needed from the image: distroless and `scratch` images are seeded too), as root so
/// `cp -a` keeps owners and modes. Empty means nothing in the claim but a `lost+found` (ext4
/// claims have one); a path the image doesn't have leaves the claim empty. Once seeded, or
/// written by the machine, a claim is never seeded again (its data survives restarts).
///
/// A `.isoloom-seeding` marker stands in the claim while the copy runs: a copy that failed (the
/// claim too small for the image's files) fails the pod, and the next try finds the marker,
/// clears what the last one left and copies again, instead of taking a half-filled claim as
/// seeded.
fn seed_container(image: &str, v: &str, path: &str) -> Value {
    let src = shell::quote(&format!("{}/.", path.trim_end_matches('/')));
    let script = format!(
        "d={SEED_DIR}; m=\"$d/{SEED_MARKER}\"; b={PROBE_DIR}/busybox; [ -d {path} ] || exit 0; for f in \"$d\"/* \"$d\"/.[!.]* \"$d\"/..?*; do if [ -e \"$f\" ] || [ -L \"$f\" ]; then case \"$f\" in \"$d/lost+found\" | \"$m\") ;; *) [ -e \"$m\" ] || exit 0; $b rm -rf \"$f\" ;; esac; fi; done; : > \"$m\" && $b cp -a {src} \"$d/\" && $b rm -f \"$m\"",
        path = shell::quote(path),
    );
    map([
        ("name", s(format!("seed-{v}"))),
        ("image", s(image)),
        ("imagePullPolicy", s("IfNotPresent")),
        ("command", list([s(format!("{PROBE_DIR}/busybox")), s("sh"), s("-c"), s(script)])),
        ("securityContext", map([("runAsUser", Value::from(0)), ("runAsGroup", Value::from(0))])),
        (
            "volumeMounts",
            list([
                map([("name", s(format!("vol-{v}"))), ("mountPath", s(SEED_DIR))]),
                map([("name", s(PROBE_VOLUME)), ("mountPath", s(PROBE_DIR)), ("readOnly", Value::Bool(true))]),
            ]),
        ),
    ])
}

pub fn generate(spec: &Spec) -> Result<Vec<GeneratedFile>, GenerateError> {
    if let Some(what) = unsupported(spec) {
        return Err(GenerateError::Unsupported {
            target: Target::Kubernetes,
            what,
        });
    }
    let ns = namespace(spec);
    let mut docs: Vec<Value> = vec![doc("Namespace", "v1", &ns, vec![])];
    let mut builds: Vec<String> = Vec::new();
    let mut scripts: Vec<String> = Vec::new();

    for (name, m) in &spec.machines {
        let Some(d) = &m.docker else { continue };
        let nets: Vec<&String> = m.networks.keys().collect();
        let image = image_of(spec, name, m);
        if let Some(b) = &d.build {
            let mut cmd = format!("docker build -t {image}");
            if let Some(f) = &d.dockerfile {
                cmd.push_str(&format!(" -f {f}"));
            }
            for (k, v) in &d.args {
                cmd.push_str(&format!(" --build-arg {}", shell::quote(&format!("{k}={v}"))));
            }
            builds.push(format!("{cmd} {b}"));
        }

        // The machine itself.
        let mut c = Mapping::new();
        c.insert(s("name"), s(name.as_str()));
        c.insert(s("image"), s(image.clone()));
        c.insert(s("imagePullPolicy"), s("IfNotPresent"));
        if m.privileged || m.read_only {
            let mut sec = Mapping::new();
            if m.privileged {
                sec.insert(s("privileged"), Value::Bool(true));
            }
            if m.read_only {
                sec.insert(s("readOnlyRootFilesystem"), Value::Bool(true));
            }
            c.insert(s("securityContext"), Value::Mapping(sec));
        }
        if m.supplied || m.docker.as_ref().is_some_and(|d| d.idle) {
            c.insert(s("command"), list([s("sleep"), s("infinity")]));
        }
        if !m.services.is_empty() {
            c.insert(
                s("ports"),
                list(
                    m.services
                        .iter()
                        .map(|sv| with_protocol(map([("containerPort", Value::from(sv.port))]), sv.protocol)),
                ),
            );
        }
        // Ready when its services answer (dormant ones aren't waited for).
        if !m.ready_ports().is_empty() {
            c.insert(
                s("readinessProbe"),
                map([
                    ("exec", map([("command", list(probe_command(m)))])),
                    ("periodSeconds", Value::from(5)),
                    ("failureThreshold", Value::from(60)),
                ]),
            );
        }
        if let Some(e) = env(spec, m) {
            c.insert(s("env"), e);
        }
        if let Some(r) = m.resources {
            let mut limits = Mapping::new();
            if let Some(cpus) = r.cpus {
                limits.insert(s("cpu"), s(cpus.to_string()));
            }
            if let Some(mb) = r.memory_mb {
                limits.insert(s("memory"), s(format!("{mb}Mi")));
            }
            if !limits.is_empty() {
                c.insert(s("resources"), map([("limits", Value::Mapping(limits))]));
            }
        }
        let mut mounts: Vec<Value> = m
            .volumes
            .iter()
            .map(|(v, path)| map([("name", s(format!("vol-{v}"))), ("mountPath", s(path.as_str()))]))
            .collect();
        // Memory-backed mounts (tmpfs) and a sized /dev/shm, as emptyDirs of medium Memory.
        for (i, path) in m.tmpfs.iter().enumerate() {
            mounts.push(map([("name", s(format!("tmpfs-{i}"))), ("mountPath", s(path.as_str()))]));
        }
        if m.shm_size.is_some() {
            mounts.push(map([("name", s("dshm")), ("mountPath", s("/dev/shm"))]));
        }
        // Isoloom's own probe (a static busybox an init container copies in): the readiness
        // probe needs nothing from the image.
        if !m.ready_ports().is_empty() {
            mounts.push(map([("name", s(PROBE_VOLUME)), ("mountPath", s(PROBE_DIR)), ("readOnly", Value::Bool(true))]));
        }
        if !mounts.is_empty() {
            c.insert(s("volumeMounts"), Value::Sequence(mounts.clone()));
        }
        let mut containers = vec![Value::Mapping(c)];

        // Its init jobs, in order, once it answers: a second container of the same pod.
        if !d.init.is_empty() {
            let mut steps = Vec::new();
            if !m.ready_ports().is_empty() {
                steps.push(format!("until {}; do sleep 2; done", probe(m)));
            }
            for script in &d.init {
                scripts.push(script.clone());
                steps.push(format!("sh /isoloom/scripts/{}", key(script)));
            }
            steps.push("exec sleep infinity".into());
            let mut j = Mapping::new();
            j.insert(s("name"), s("init"));
            j.insert(s("image"), s(image.clone()));
            j.insert(s("imagePullPolicy"), s("IfNotPresent"));
            j.insert(s("command"), list([s("sh"), s("-c"), s(steps.join(" && "))]));
            if let Some(e) = env(spec, m) {
                j.insert(s("env"), e);
            }
            let mut jm = mounts.clone();
            jm.push(map([("name", s("scripts")), ("mountPath", s("/isoloom/scripts"))]));
            j.insert(s("volumeMounts"), Value::Sequence(jm));
            containers.push(Value::Mapping(j));
        }

        let mut pod = Mapping::new();
        pod.insert(s("hostname"), s(name.as_str()));
        if let Some(affinity) = beside_owners(spec, name, m) {
            pod.insert(s("affinity"), affinity);
        }
        // Namespaced sysctls are the pod's (the kubelet allows the safe ones; others need its
        // --allowed-unsafe-sysctls).
        if !m.sysctls.is_empty() {
            let sysctls = m.sysctls.iter().map(|(k, v)| map([("name", s(k.as_str())), ("value", s(v.as_str()))]));
            pod.insert(s("securityContext"), map([("sysctls", list(sysctls))]));
        }
        // Wait for the machines it depends on: their services answer.
        let mut waits: Vec<Value> = Vec::new();
        // Isoloom's busybox: for the readiness probe and for seeding the volumes.
        let needs_probe = !m.ready_ports().is_empty() || !m.volumes.is_empty();
        if needs_probe {
            waits.push(map([
                ("name", s(PROBE_VOLUME)),
                ("image", s(PROBE_IMAGE)),
                ("command", list([s("/bin/cp"), s("/bin/busybox"), s("/probe/busybox")])),
                ("volumeMounts", list([map([("name", s(PROBE_VOLUME)), ("mountPath", s("/probe"))])])),
            ]));
        }
        // The volumes start with the image's files (before the machine and its init jobs).
        // A shared volume is seeded once, by its owner.
        waits.extend(
            m.volumes
                .iter()
                .filter(|(v, _)| holds(spec, name, v))
                .map(|(v, path)| seed_container(&image, v, path)),
        );
        waits.extend(m.depends_on.iter().filter_map(|dep| {
            let ports: Vec<u16> = spec.machines[dep].ready_ports();
            (!ports.is_empty()).then(|| {
                let cond = ports.iter().map(|p| format!("nc -z {dep} {p}")).collect::<Vec<_>>().join(" && ");
                map([
                    ("name", s(format!("wait-{dep}"))),
                    ("image", s(UTILITY_IMAGE)),
                    ("command", list([s("sh"), s("-c"), s(format!("until {cond}; do sleep 2; done"))])),
                ])
            })
        }));
        if !waits.is_empty() {
            pod.insert(s("initContainers"), Value::Sequence(waits));
        }
        pod.insert(s("containers"), Value::Sequence(containers));
        // Schedule the pod on a node of the machine's architecture.
        pod.insert(s("nodeSelector"), map([("kubernetes.io/arch", s(m.arch.id()))]));
        // Custom resolver (e.g. the lab's domain controller): servers and search domains.
        if let Some(dns) = &m.dns
            && (!dns.servers.is_empty() || !dns.search.is_empty())
        {
            pod.insert(s("dnsPolicy"), s("None"));
            let mut cfg = Mapping::new();
            if !dns.servers.is_empty() {
                cfg.insert(s("nameservers"), list(dns.servers.iter().map(|d| s(d.as_str()))));
            }
            if !dns.search.is_empty() {
                cfg.insert(s("searches"), list(dns.search.iter().map(|d| s(d.as_str()))));
            }
            pod.insert(s("dnsConfig"), Value::Mapping(cfg));
        }
        let mut volumes: Vec<Value> = m
            .volumes
            .keys()
            .map(|v| {
                map([
                    ("name", s(format!("vol-{v}"))),
                    ("persistentVolumeClaim", map([("claimName", s(spec.volume_key(name, v)))])),
                ])
            })
            .collect();
        if !d.init.is_empty() {
            volumes.push(map([
                ("name", s("scripts")),
                ("configMap", map([("name", s(SCRIPTS)), ("defaultMode", Value::from(0o755))])),
            ]));
        }
        if needs_probe {
            volumes.push(map([("name", s(PROBE_VOLUME)), ("emptyDir", Value::Mapping(Default::default()))]));
        }
        for (i, _) in m.tmpfs.iter().enumerate() {
            volumes.push(map([("name", s(format!("tmpfs-{i}"))), ("emptyDir", map([("medium", s("Memory"))]))]));
        }
        if let Some(shm) = &m.shm_size {
            volumes.push(map([
                ("name", s("dshm")),
                ("emptyDir", map([("medium", s("Memory")), ("sizeLimit", s(shm.as_str()))])),
            ]));
        }
        if !volumes.is_empty() {
            pod.insert(s("volumes"), Value::Sequence(volumes));
        }
        docs.push(doc(
            "Deployment",
            "apps/v1",
            name,
            vec![(
                "spec",
                map([
                    ("replicas", Value::from(1)),
                    ("strategy", map([("type", s("Recreate"))])),
                    ("selector", map([("matchLabels", labels(&[(MACHINE_LABEL.to_string(), name.as_str())]))])),
                    (
                        "template",
                        map([("metadata", map([("labels", pod_labels(Some(name), &nets))])), ("spec", Value::Mapping(pod))]),
                    ),
                ]),
            )],
        ));

        for v in m.volumes.keys().filter(|v| holds(spec, name, v)) {
            docs.push(doc(
                "PersistentVolumeClaim",
                "v1",
                &spec.volume_key(name, v),
                vec![(
                    "spec",
                    map([
                        ("accessModes", list([s("ReadWriteOnce")])),
                        ("resources", map([("requests", map([("storage", s("1Gi"))]))])),
                    ]),
                )],
            ));
        }

        // Its name, for the other machines.
        if !m.services.is_empty() {
            docs.push(doc(
                "Service",
                "v1",
                name,
                vec![(
                    "spec",
                    map([
                        ("selector", labels(&[(MACHINE_LABEL.to_string(), name.as_str())])),
                        (
                            "ports",
                            list(m.services.iter().map(|sv| {
                                with_protocol(
                                    map([
                                        ("name", s(port_name(sv.port, sv.protocol))),
                                        ("port", Value::from(sv.port)),
                                        ("targetPort", Value::from(sv.port)),
                                    ]),
                                    sv.protocol,
                                )
                            })),
                        ),
                    ]),
                )],
            ));
        }
        // Published services: reachable from outside the cluster.
        let published: Vec<_> = m.services.iter().filter_map(|sv| sv.publish.map(|h| (sv.port, h, sv.protocol))).collect();
        if !published.is_empty() {
            docs.push(doc(
                "Service",
                "v1",
                &format!("{name}-published"),
                vec![(
                    "spec",
                    map([
                        ("type", s("LoadBalancer")),
                        ("selector", labels(&[(MACHINE_LABEL.to_string(), name.as_str())])),
                        (
                            "ports",
                            list(published.iter().map(|(p, h, proto)| {
                                with_protocol(
                                    map([("name", s(port_name(*h, *proto))), ("port", Value::from(*h)), ("targetPort", Value::from(*p))]),
                                    *proto,
                                )
                            })),
                        ),
                    ]),
                )],
            ));
            docs.push(doc(
                "NetworkPolicy",
                "networking.k8s.io/v1",
                &format!("publish-{name}"),
                vec![(
                    "spec",
                    map([
                        ("podSelector", map([("matchLabels", labels(&[(MACHINE_LABEL.to_string(), name.as_str())]))])),
                        (
                            "ingress",
                            list([map([
                                ("from", list([map([("ipBlock", map([("cidr", s("0.0.0.0/0"))]))])])),
                                (
                                    "ports",
                                    list(published.iter().map(|(p, _, proto)| with_protocol(map([("port", Value::from(*p))]), *proto))),
                                ),
                            ])]),
                        ),
                    ]),
                )],
            ));
        }
        // Offline: nothing new out of the environment (DNS stays).
        if offline(spec, name, m) {
            docs.push(offline_policy(
                &format!("offline-{name}"),
                labels(&[(MACHINE_LABEL.to_string(), name.as_str())]),
            ));
        }
    }

    // Networks: deny by default, then each network's machines reach each other, then `reach`.
    docs.push(doc(
        "NetworkPolicy",
        "networking.k8s.io/v1",
        "isoloom-default-deny",
        vec![(
            "spec",
            map([("podSelector", Value::Mapping(Mapping::new())), ("policyTypes", list([s("Ingress")]))]),
        )],
    ));
    for net in spec.networks.keys() {
        let sel = map([("matchLabels", labels(&[(net_label(net), "member")]))]);
        docs.push(doc(
            "NetworkPolicy",
            "networking.k8s.io/v1",
            &format!("net-{net}"),
            vec![(
                "spec",
                map([
                    ("podSelector", sel.clone()),
                    ("ingress", list([map([("from", list([map([("podSelector", sel)])]))])])),
                ]),
            )],
        ));
    }
    for (i, r) in spec.reach.iter().enumerate() {
        let mut rule = Mapping::new();
        rule.insert(
            s("from"),
            list([map([("podSelector", map([("matchLabels", labels(&[(net_label(&r.from), "member")]))]))])]),
        );
        if !r.ports.is_empty() {
            rule.insert(
                s("ports"),
                list(r.ports.iter().flat_map(|p| {
                    [
                        map([("port", Value::from(*p)), ("protocol", s("TCP"))]),
                        map([("port", Value::from(*p)), ("protocol", s("UDP"))]),
                    ]
                })),
            );
        }
        docs.push(doc(
            "NetworkPolicy",
            "networking.k8s.io/v1",
            // The index keeps the name unique: `reach-{from}-{to}` alone collides when a network
            // name contains a dash (reach a-b→c and a→b-c would both be `reach-a-b-c`).
            &format!("reach-{i}-{}-{}", r.from, r.to),
            vec![(
                "spec",
                map([
                    ("podSelector", map([("matchLabels", labels(&[(net_label(&r.to), "member")]))])),
                    ("ingress", list([Value::Mapping(rule)])),
                ]),
            )],
        ));
    }

    // Checks: a Job per position, standing where that machine stands (its network labels, so
    // the same policies apply), else on every network. Each runner is a sh script next to the
    // Job, carried by a ConfigMap of its own.
    let mut check_files = Vec::new();
    // The same checks for the chart: its test Jobs and their runners.
    let mut chart_tests: Vec<Value> = Vec::new();
    let mut chart_runners: Vec<(String, String)> = Vec::new();
    let plan = checks::plan(spec);
    if !plan.is_empty() {
        let default_pos = checks::default_position(spec);
        // Kubernetes picks pod addresses: machines are reached by name (their Service).
        let host = |h: &checks::Host, _: &checks::Position| -> String {
            match h {
                checks::Host::Literal(l) => l.clone(),
                checks::Host::Machine { name, .. } => name.clone(),
            }
        };
        let run_script = |path: &str| format!("sh /isoloom/scripts/{}", key(path));
        let render = checks::Render {
            host: &host,
            script: &run_script,
            playbook: None,
        };
        let mut jobs: Vec<Value> = Vec::new();
        let mut runner_scripts: Vec<String> = Vec::new();
        for (pos, group) in checks::by_position(spec, &plan) {
            let id = pos.id();
            let job_name = if pos == default_pos {
                "isoloom-check".to_string()
            } else {
                format!("isoloom-check-{id}")
            };
            let nets: Vec<&String> = match &pos {
                checks::Position::Machine(m) => spec.machines[m].networks.keys().collect(),
                checks::Position::Networks => spec.networks.keys().collect(),
            };
            for c in &group {
                if let checks::Probe::Script { path } = &c.probe {
                    scripts.push(path.clone());
                }
            }
            // `exec` checks run inside the machine (`kubectl exec -i deploy/<machine> -- sh -s`),
            // from their own runner; the Job beside it doesn't run them.
            let (execs, group): (Vec<&checks::Resolved>, Vec<&checks::Resolved>) =
                group.into_iter().partition(|c| matches!(c.probe, checks::Probe::Exec { .. }));
            if !execs.is_empty() {
                check_files.push(GeneratedFile {
                    path: format!("{OUTPUT_DIR}/{DIR}/checks/{}", super::docker::exec_runner(id)),
                    contents: checks::script(&pos, &execs, &render),
                });
            }
            let runner = checks::script(&pos, &group, &render);
            chart_runners.push((format!("{id}.sh"), runner.clone()));
            check_files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/{id}.sh"),
                contents: runner,
            });
            runner_scripts.push(format!("{id}.sh"));
            let mut labels_ = pod_labels(None, &nets);
            if let Value::Mapping(m) = &mut labels_ {
                m.insert(s(CHECK_LABEL), s(job_name.as_str()));
            }
            // Standing on offline networks, the checks are offline too (as a machine there would be).
            if !nets.is_empty() && nets.iter().all(|n| !spec.networks[n.as_str()].internet) {
                docs.push(offline_policy(
                    &format!("offline-{job_name}"),
                    labels(&[(CHECK_LABEL.to_string(), job_name.as_str())]),
                ));
            }
            let mut mounts = vec![map([("name", s("runner")), ("mountPath", s("/isoloom/run"))])];
            let mut vols = vec![map([
                ("name", s("runner")),
                ("configMap", map([("name", s(CHECK_SCRIPTS)), ("defaultMode", Value::from(0o755))])),
            ])];
            if group.iter().any(|c| matches!(c.probe, checks::Probe::Script { .. })) {
                mounts.push(map([("name", s("scripts")), ("mountPath", s("/isoloom/scripts"))]));
                vols.push(map([
                    ("name", s("scripts")),
                    ("configMap", map([("name", s(SCRIPTS)), ("defaultMode", Value::from(0o755))])),
                ]));
            }
            jobs.push(doc(
                "Job",
                "batch/v1",
                &job_name,
                vec![(
                    "spec",
                    map([
                        ("backoffLimit", Value::from(0)),
                        (
                            "template",
                            map([
                                ("metadata", map([("labels", labels_)])),
                                (
                                    "spec",
                                    map([
                                        ("restartPolicy", s("Never")),
                                        (
                                            "containers",
                                            list([map([
                                                ("name", s("check")),
                                                ("image", s(CHECK_IMAGE)),
                                                ("command", list([s("/bin/sh"), s(format!("/isoloom/run/{id}.sh"))])),
                                                ("volumeMounts", list(mounts)),
                                            ])]),
                                        ),
                                        ("volumes", list(vols)),
                                    ]),
                                ),
                            ]),
                        ),
                    ]),
                )],
            ));
        }
        chart_tests = jobs.clone();
        let job_names: Vec<String> = jobs.iter().filter_map(|j| j["metadata"]["name"].as_str().map(str::to_string)).collect();
        let mut out = header("#");
        out.push_str(&format!(
            "# The environment's checks, one Job per position. Run after the environment (or `isoloom test kubernetes`):\n#   kubectl -n {ns} delete job {names} --ignore-not-found\n#   kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes/checks | kubectl apply -f -\n#   kubectl -n {ns} wait --for=condition=complete job --all --timeout=300s; kubectl -n {ns} logs -l {CHECK_LABEL}\n",
            names = job_names.join(" ")
        ));
        let body: Vec<String> = jobs.iter().map(yaml).collect();
        out.push_str(&body.join("---\n"));
        check_files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/job.yaml"),
            contents: out,
        });
        let mut k = header("#");
        k.push_str(&format!(
            "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nnamespace: {ns}\nresources:\n  - job.yaml\ngeneratorOptions:\n  disableNameSuffixHash: true\nconfigMapGenerator:\n  - name: {CHECK_SCRIPTS}\n    files:\n"
        ));
        for f in &runner_scripts {
            let _ = writeln!(k, "      - {f}");
        }
        check_files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/kustomization.yaml"),
            contents: k,
        });
    }

    // The manifests.
    let mut out = header("#");
    let _ = writeln!(
        out,
        "# Start:  kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes | kubectl apply -f -\n#         kubectl -n {ns} wait --for=condition=Available deployment --all --timeout=600s\n# Stop:   kubectl delete namespace {ns}"
    );
    if !builds.is_empty() {
        out.push_str("# Images built from the project, before starting (a local cluster sharing Docker's images:\n# OrbStack, Docker Desktop; elsewhere, push them to a registry the cluster pulls from):\n");
        for b in &builds {
            let _ = writeln!(out, "#   {b}");
        }
    }
    if spec.machines.values().any(|m| !m.inputs.is_empty()) {
        let names: Vec<&String> = spec.inputs.iter().collect();
        let _ = writeln!(
            out,
            "# Inputs (optional, unset ones are empty): kubectl -n {ns} create secret generic {INPUTS} {}",
            names.iter().map(|n| format!("--from-literal={n}=…")).collect::<Vec<_>>().join(" ")
        );
    }
    out.push('\n');
    let body: Vec<String> = docs.iter().map(yaml).collect();
    out.push_str(&body.join("---\n"));

    let mut k = header("#");
    let _ = write!(
        k,
        "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nnamespace: {ns}\nresources:\n  - environment.yaml\n"
    );
    scripts.sort();
    scripts.dedup();
    if !scripts.is_empty() {
        let _ = write!(
            k,
            "# The project's scripts the pods run (init jobs, checks), read from the project at apply time.\ngeneratorOptions:\n  disableNameSuffixHash: true\nconfigMapGenerator:\n  - name: {SCRIPTS}\n    files:\n"
        );
        for f in &scripts {
            let _ = writeln!(k, "      - {}={ROOT}/{f}", key(f));
        }
    }

    let mut files = vec![
        GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/kustomization.yaml"),
            contents: k,
        },
        GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/environment.yaml"),
            contents: out,
        },
    ];
    files.extend(check_files);
    files.extend(chart(spec, &docs, &scripts, &chart_tests, &chart_runners));
    Ok(files)
}

/// Where the chart is, and where its copies of the project's scripts go.
pub const CHART: &str = "chart";
pub const CHART_PROJECT_FILES: &str = "files/project";

/// Helm's template engine reads `{{` as an action: one already in a manifest (a script, the
/// message) is printed as it is.
fn helm_literal(text: &str) -> String {
    text.replace("{{", "{{ \"{{\" }}")
}

/// The environment as a Helm chart: the same manifests (the namespace is the release's), each
/// machine's image a value (`images.<machine>`), the inputs a Secret from `inputs.<NAME>`, the
/// checks `helm test` Pods (the `exec` ones, run inside a machine, stay with `isoloom test`),
/// the project's scripts read from `files/project/` (see [`crate::project_copies`]).
fn chart(spec: &Spec, docs: &[Value], scripts: &[String], tests: &[Value], runners: &[(String, String)]) -> Vec<GeneratedFile> {
    let base = format!("{OUTPUT_DIR}/{DIR}/{CHART}");
    let constraints = super::constraints::of(spec);
    let token = |machine: &str| format!("isoloom-chart-image--{machine}--");

    // Each machine's image, pulled through the constraints' mirrors (a built one is local).
    let mut images = Mapping::new();
    let mut original: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
    for (name, m) in &spec.machines {
        let Some(d) = &m.docker else { continue };
        let image = image_of(spec, name, m);
        let pulled = match (constraints, &d.build) {
            (Some(c), None) => super::constraints::rewrite_image(c, &image),
            _ => image.clone(),
        };
        images.insert(s(name.as_str()), s(pulled));
        original.insert(name.clone(), image);
    }

    // The manifests, without the namespace, a machine's own image swapped for its value.
    let manifests: Vec<String> = docs
        .iter()
        .filter(|d| d["kind"].as_str() != Some("Namespace"))
        .map(|d| {
            let mut d = d.clone();
            if d["kind"].as_str() == Some("Deployment")
                && let Some(name) = d["metadata"]["name"].as_str().map(str::to_string)
                && let Some(image) = original.get(&name)
                && let Some(Value::Mapping(pod)) = d.get_mut("spec").and_then(|v| v.get_mut("template")).and_then(|v| v.get_mut("spec"))
            {
                for key in ["containers", "initContainers"] {
                    if let Some(Value::Sequence(cs)) = pod.get_mut(s(key)) {
                        for c in cs.iter_mut() {
                            if c["image"].as_str() == Some(image.as_str())
                                && let Value::Mapping(c) = c
                            {
                                c.insert(s("image"), s(token(&name)));
                            }
                        }
                    }
                }
            }
            yaml(&d)
        })
        .collect();
    let mut env = header("#");
    env.push_str(&helm_literal(&manifests.join("---\n")));
    for name in original.keys() {
        env = env.replace(
            &format!("image: {}", token(name)),
            &format!("image: {{{{ index .Values.images {} | quote }}}}", serde_json::to_string(name).expect("a name")),
        );
    }

    let mut files = Vec::new();
    let description = format!("The {} environment, generated by Isoloom from isoloom.yml", spec.name);
    files.push(GeneratedFile {
        path: format!("{base}/Chart.yaml"),
        contents: format!(
            "{}apiVersion: v2\nname: {}\ndescription: {}\ntype: application\n# `isoloom chart --version <v>` packages it with that version.\nversion: 0.1.0\nappVersion: \"0.1.0\"\n",
            header("#"),
            spec.name,
            serde_json::to_string(&description).expect("text")
        ),
    });

    let mut values = Mapping::new();
    values.insert(s("images"), Value::Mapping(images));
    let has_inputs = spec.machines.values().any(|m| !m.inputs.is_empty());
    if has_inputs {
        let mut inputs = Mapping::new();
        for i in &spec.inputs {
            inputs.insert(s(i.as_str()), s(""));
        }
        values.insert(s("inputs"), Value::Mapping(inputs));
    }
    let mut v = header("#");
    v.push_str("# images: each machine's image (images built from the project must be pushed where the\n# cluster pulls from, and named here). inputs: the launch-time inputs (unset ones are empty).\n");
    v.push_str(&yaml(&Value::Mapping(values)));
    files.push(GeneratedFile {
        path: format!("{base}/values.yaml"),
        contents: v,
    });

    files.push(GeneratedFile {
        path: format!("{base}/templates/environment.yaml"),
        contents: env,
    });

    if !scripts.is_empty() {
        let mut t = header("#");
        let _ = writeln!(
            t,
            "# The project's scripts (init jobs, checks): copies in {CHART_PROJECT_FILES}/, written by `isoloom generate`."
        );
        let _ = writeln!(t, "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {SCRIPTS}\ndata:");
        for f in scripts {
            let k = key(f);
            let _ = writeln!(
                t,
                "  {k}: {{{{ required \"{CHART_PROJECT_FILES}/{k} is missing: run isoloom generate\" (.Files.Get \"{CHART_PROJECT_FILES}/{k}\") | quote }}}}"
            );
        }
        files.push(GeneratedFile {
            path: format!("{base}/templates/scripts.yaml"),
            contents: t,
        });
    }

    if has_inputs {
        let mut t = header("#");
        let _ = writeln!(
            t,
            "apiVersion: v1\nkind: Secret\nmetadata:\n  name: {INPUTS}\ntype: Opaque\nstringData:\n{{{{- range $name, $value := .Values.inputs }}}}\n  {{{{ $name }}}}: {{{{ $value | toString | quote }}}}\n{{{{- end }}}}"
        );
        files.push(GeneratedFile {
            path: format!("{base}/templates/inputs.yaml"),
            contents: t,
        });
    }

    if !tests.is_empty() {
        let mut t = header("#");
        t.push_str(
            "# The checks: `helm test <release>` runs a Job per position (`exec` checks, run inside a machine,\n# stay with `isoloom test kubernetes`).\n",
        );
        let _ = writeln!(t, "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {CHECK_SCRIPTS}\ndata:");
        for (name, _) in runners {
            let _ = writeln!(t, "  {name}: {{{{ .Files.Get \"files/checks/{name}\" | quote }}}}");
        }
        // As Pods named after their Job: `helm test --logs` reads each test's logs from a pod
        // of the hook's own name.
        for job in tests {
            let name = job["metadata"]["name"].as_str().unwrap_or("isoloom-check");
            let template = &job["spec"]["template"];
            let mut meta = Mapping::new();
            meta.insert(s("name"), s(name));
            if let Some(l) = template["metadata"].get("labels") {
                meta.insert(s("labels"), l.clone());
            }
            meta.insert(
                s("annotations"),
                map([("helm.sh/hook", s("test")), ("helm.sh/hook-delete-policy", s("before-hook-creation"))]),
            );
            let mut pod = Mapping::new();
            pod.insert(s("apiVersion"), s("v1"));
            pod.insert(s("kind"), s("Pod"));
            pod.insert(s("metadata"), Value::Mapping(meta));
            pod.insert(s("spec"), template["spec"].clone());
            t.push_str("---\n");
            t.push_str(&helm_literal(&yaml(&Value::Mapping(pod))));
        }
        files.push(GeneratedFile {
            path: format!("{base}/templates/tests/checks.yaml"),
            contents: t,
        });
        for (name, contents) in runners {
            files.push(GeneratedFile {
                path: format!("{base}/files/checks/{name}"),
                contents: contents.clone(),
            });
        }
    }

    // The message as written when it has no placeholders: filling them takes the resolved
    // snapshot, which asks every generator (this one too) whether it would refuse the spec.
    let mut notes = String::new();
    match spec.message.as_deref().map(str::trim_end).filter(|m| !m.trim().is_empty()) {
        Some(m) if !m.contains("{{") => {
            notes.push_str(&helm_literal(m));
            notes.push_str("\n\n");
        }
        Some(_) => notes.push_str("How to use it: isoloom message --target kubernetes (in the project).\n\n"),
        None => {}
    }
    let _ = writeln!(notes, "Checks: helm test {{{{ .Release.Name }}}} -n {{{{ .Release.Namespace }}}}");
    files.push(GeneratedFile {
        path: format!("{base}/templates/NOTES.txt"),
        contents: notes,
    });
    files
}

fn yaml(v: &Value) -> String {
    serde_yaml_ng::to_string(v).expect("a kubernetes manifest serializes")
}

/// A port entry with its `protocol`: none for TCP (the default, kept out), `UDP`.
fn with_protocol(mut v: Value, p: Protocol) -> Value {
    if let Value::Mapping(m) = &mut v
        && !p.is_tcp()
    {
        m.insert(s("protocol"), s("UDP"));
    }
    v
}

/// A Service port's name, unique within the Service: `p53`, and `p53-udp` for UDP (a port
/// served over both protocols is two entries).
fn port_name(port: u16, p: Protocol) -> String {
    if p.is_tcp() { format!("p{port}") } else { format!("p{port}-udp") }
}
