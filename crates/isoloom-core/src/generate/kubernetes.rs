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
//! - Checks: a Job on the access machine's networks, in `.isoloom/kubernetes/checks/`.
//! - Addresses: Kubernetes picks pod addresses; names, ports and reachability are kept.

use std::fmt::Write;

use serde_yaml_ng::{Mapping, Value};

use super::docker::{CHECK_IMAGE, UTILITY_IMAGE, image_of, list, map, offline, probe, s};
use super::{GenerateError, GeneratedFile, OUTPUT_DIR, header};
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

/// The check pods' labels: where they stand, and that they're the checks.
fn check_labels(nets: &[&String]) -> Value {
    let mut l = pod_labels(None, nets);
    if let Value::Mapping(m) = &mut l {
        m.insert(s(CHECK_LABEL), s("runner"));
    }
    l
}

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
    if spec.checks.iter().any(|c| c.ends_with(".yml") || c.ends_with(".yaml")) {
        return Some("Ansible checks (.yml) run on VM targets for now".into());
    }
    if spec.networks.values().any(|n| n.gateway.is_some()) {
        return Some("networks with a `gateway` machine on Kubernetes come later".into());
    }
    None
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
            builds.push(format!("docker build -t {image} {b}"));
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
        if m.supplied {
            c.insert(s("command"), list([s("sleep"), s("infinity")]));
        }
        if !m.services.is_empty() {
            c.insert(s("ports"), list(m.services.iter().map(|sv| map([("containerPort", Value::from(sv.port))]))));
            c.insert(
                s("readinessProbe"),
                map([
                    ("exec", map([("command", list([s("sh"), s("-c"), s(probe(m))]))])),
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
        let mounts: Vec<Value> = m
            .volumes
            .iter()
            .map(|(v, path)| map([("name", s(format!("vol-{v}"))), ("mountPath", s(path.as_str()))]))
            .collect();
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
        let waits: Vec<Value> = m
            .depends_on
            .iter()
            .filter_map(|dep| {
                let ports: Vec<u16> = spec.machines[dep].services.iter().map(|sv| sv.port).collect();
                (!ports.is_empty()).then(|| {
                    let cond = ports.iter().map(|p| format!("nc -z {dep} {p}")).collect::<Vec<_>>().join(" && ");
                    map([
                        ("name", s(format!("wait-{dep}"))),
                        ("image", s(UTILITY_IMAGE)),
                        ("command", list([s("sh"), s("-c"), s(format!("until {cond}; do sleep 2; done"))])),
                    ])
                })
            })
            .collect();
        if !waits.is_empty() {
            pod.insert(s("initContainers"), Value::Sequence(waits));
        }
        pod.insert(s("containers"), Value::Sequence(containers));
        // Schedule the pod on a node of the machine's architecture.
        pod.insert(s("nodeSelector"), map([("kubernetes.io/arch", s(m.arch.id()))]));
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
    for r in &spec.reach {
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
            &format!("reach-{}-{}", r.from, r.to),
            vec![(
                "spec",
                map([
                    ("podSelector", map([("matchLabels", labels(&[(net_label(&r.to), "member")]))])),
                    ("ingress", list([Value::Mapping(rule)])),
                ]),
            )],
        ));
    }

    // Checks: a Job standing where the access machine stands (its networks), else on all.
    let mut check_files = Vec::new();
    if !spec.checks.is_empty() {
        let access_nets: Vec<&String> = match spec.machines.values().find(|m| m.access) {
            Some(a) => a.networks.keys().collect(),
            None => spec.networks.keys().collect(),
        };
        // Standing on offline networks, the checks are offline too (as a machine there would be).
        if !access_nets.is_empty() && access_nets.iter().all(|n| !spec.networks[n.as_str()].internet) {
            docs.push(offline_policy("offline-isoloom-check", labels(&[(CHECK_LABEL.to_string(), "runner")])));
        }
        let mut run = Vec::new();
        for c in &spec.checks {
            scripts.push(c.clone());
            run.push(format!("echo '== {c}' && sh /isoloom/scripts/{}", key(c)));
        }
        let job = doc(
            "Job",
            "batch/v1",
            "isoloom-check",
            vec![(
                "spec",
                map([
                    ("backoffLimit", Value::from(0)),
                    (
                        "template",
                        map([
                            ("metadata", map([("labels", check_labels(&access_nets))])),
                            (
                                "spec",
                                map([
                                    ("restartPolicy", s("Never")),
                                    (
                                        "containers",
                                        list([map([
                                            ("name", s("check")),
                                            ("image", s(CHECK_IMAGE)),
                                            ("command", list([s("/bin/sh"), s("-c"), s(run.join(" && "))])),
                                            ("volumeMounts", list([map([("name", s("scripts")), ("mountPath", s("/isoloom/scripts"))])])),
                                        ])]),
                                    ),
                                    (
                                        "volumes",
                                        list([map([
                                            ("name", s("scripts")),
                                            ("configMap", map([("name", s(SCRIPTS)), ("defaultMode", Value::from(0o755))])),
                                        ])]),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            )],
        );
        let mut checks = header("#");
        checks.push_str(&format!(
            "# The environment's checks, from the access machine's networks. Run after the environment:\n#   kubectl -n {ns} delete job isoloom-check --ignore-not-found\n#   kubectl kustomize --load-restrictor LoadRestrictionsNone .isoloom/kubernetes/checks | kubectl apply -f -\n#   kubectl -n {ns} wait --for=condition=complete job/isoloom-check --timeout=300s; kubectl -n {ns} logs job/isoloom-check\n"
        ));
        checks.push_str(&yaml(&job));
        check_files.push(GeneratedFile {
            path: format!("{OUTPUT_DIR}/{DIR}/checks/job.yaml"),
            contents: checks,
        });
        let mut k = header("#");
        k.push_str(&format!(
            "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nnamespace: {ns}\nresources:\n  - job.yaml\n"
        ));
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
