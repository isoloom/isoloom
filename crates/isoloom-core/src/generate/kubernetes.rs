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
//! - Volumes: a PersistentVolumeClaim each. `init:` jobs: a container in the machine's pod
//!   (same network and volumes), run once the machine answers. `depends_on`: init containers
//!   that wait for the dependencies' services.
//! - Checks: a Job per position in `.isoloom/kubernetes/checks/`, with the networks (labels) of
//!   the machine it stands for, so the same policies apply to it.
//! - Addresses: Kubernetes picks pod addresses; names, ports and reachability are kept.

use std::fmt::Write;

use serde_yaml_ng::{Mapping, Value};

use super::docker::{CHECK_IMAGE, PROBE_DIR, PROBE_IMAGE, UTILITY_IMAGE, image_of, list, map, offline, probe, probe_command, s};
use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
use crate::checks;
use crate::model::{Machine, Spec, Target};

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

/// The machine's inputs, from the `isoloom-inputs` Secret (optional: unset inputs are empty).
fn env(m: &Machine) -> Option<Value> {
    if m.inputs.is_empty() {
        return None;
    }
    Some(list(m.inputs.iter().map(|i| {
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
    })))
}

fn unsupported(spec: &Spec) -> Option<String> {
    if !spec.provision.is_empty() {
        return Some("environment-level provisioning (`provision:`) runs on VM targets for now".into());
    }
    if let Some(what) = super::container_checks_unsupported(spec) {
        return Some(what);
    }
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine on Kubernetes come later".into());
    }
    if spec.networks.values().any(|n| n.tc.is_some()) {
        return Some("link impairment (`tc`) needs a router in the path; Kubernetes has none".into());
    }
    None
}

/// The emptyDir holding Isoloom's probe, and the init container filling it.
const PROBE_VOLUME: &str = "isoloom-probe";

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
                cmd.push_str(&format!(" --build-arg {}", super::cloud_vm::sh_quote(&format!("{k}={v}"))));
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
            c.insert(s("ports"), list(m.services.iter().map(|sv| map([("containerPort", Value::from(sv.port))]))));
            c.insert(
                s("readinessProbe"),
                map([
                    ("exec", map([("command", list(probe_command(m)))])),
                    ("periodSeconds", Value::from(5)),
                    ("failureThreshold", Value::from(60)),
                ]),
            );
        }
        if let Some(e) = env(m) {
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
        if !m.services.is_empty() {
            mounts.push(map([("name", s(PROBE_VOLUME)), ("mountPath", s(PROBE_DIR)), ("readOnly", Value::Bool(true))]));
        }
        if !mounts.is_empty() {
            c.insert(s("volumeMounts"), Value::Sequence(mounts.clone()));
        }
        let mut containers = vec![Value::Mapping(c)];

        // Its init jobs, in order, once it answers: a second container of the same pod.
        if !d.init.is_empty() {
            let mut steps = Vec::new();
            if !m.services.is_empty() {
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
            if let Some(e) = env(m) {
                j.insert(s("env"), e);
            }
            let mut jm = mounts.clone();
            jm.push(map([("name", s("scripts")), ("mountPath", s("/isoloom/scripts"))]));
            j.insert(s("volumeMounts"), Value::Sequence(jm));
            containers.push(Value::Mapping(j));
        }

        let mut pod = Mapping::new();
        pod.insert(s("hostname"), s(name.as_str()));
        // Wait for the machines it depends on: their services answer.
        let mut waits: Vec<Value> = Vec::new();
        if !m.services.is_empty() {
            waits.push(map([
                ("name", s(PROBE_VOLUME)),
                ("image", s(PROBE_IMAGE)),
                ("command", list([s("/bin/cp"), s("/bin/busybox"), s("/probe/busybox")])),
                ("volumeMounts", list([map([("name", s(PROBE_VOLUME)), ("mountPath", s("/probe"))])])),
            ]));
        }
        waits.extend(m.depends_on.iter().filter_map(|dep| {
            let ports: Vec<u16> = spec.machines[dep].services.iter().map(|sv| sv.port).collect();
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
                    ("persistentVolumeClaim", map([("claimName", s(format!("{name}-{v}")))])),
                ])
            })
            .collect();
        if !d.init.is_empty() {
            volumes.push(map([
                ("name", s("scripts")),
                ("configMap", map([("name", s(SCRIPTS)), ("defaultMode", Value::from(0o755))])),
            ]));
        }
        if !m.services.is_empty() {
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

        for v in m.volumes.keys() {
            docs.push(doc(
                "PersistentVolumeClaim",
                "v1",
                &format!("{name}-{v}"),
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
                                map([
                                    ("name", s(format!("p{}", sv.port))),
                                    ("port", Value::from(sv.port)),
                                    ("targetPort", Value::from(sv.port)),
                                ])
                            })),
                        ),
                    ]),
                )],
            ));
        }
        // Published services: reachable from outside the cluster.
        let published: Vec<_> = m.services.iter().filter_map(|sv| sv.publish.map(|h| (sv.port, h))).collect();
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
                            list(
                                published
                                    .iter()
                                    .map(|(p, h)| map([("name", s(format!("p{h}"))), ("port", Value::from(*h)), ("targetPort", Value::from(*p))])),
                            ),
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
                                ("ports", list(published.iter().map(|(p, _)| map([("port", Value::from(*p))])))),
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
                    path: format!("{OUTPUT_DIR}/{DIR}/checks/{}", super::docker::exec_runner(&id)),
                    contents: checks::script(&pos, &execs, &render),
                });
            }
            check_files.push(GeneratedFile {
                path: format!("{OUTPUT_DIR}/{DIR}/checks/{id}.sh"),
                contents: checks::script(&pos, &group, &render),
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
    Ok(files)
}

fn yaml(v: &Value) -> String {
    serde_yaml_ng::to_string(v).expect("a kubernetes manifest serializes")
}
