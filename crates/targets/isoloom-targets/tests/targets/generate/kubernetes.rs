//! The `kubernetes` target: network policies, volumes seeded from the image, published and offline
//! machines.

use isoloom_spec::Target;
use isoloom_targets::generate;
use isoloom_test_support::{manifest, manifest_names, named, yaml_at, yaml_documents, yaml_str, yaml_strs};
use serde_yaml_ng::Value;

use crate::testing::{Outcome, allowed, example, file, generated, lab};

/// The manifests of `environment.yaml` that `lab` generates, parsed, with its text (for the
/// header comments).
fn environment(lab: &isoloom_targets::Lab) -> Outcome<(Vec<Value>, String)> {
    let text = generated(lab, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    Ok((yaml_documents(&text), text))
}

/// The container `name` of Deployment `name`'s pod.
fn container<'d>(docs: &'d [Value], name: &str) -> &'d Value {
    named(manifest(docs, "Deployment", name), "spec.template.spec.containers", name)
}

#[test]
fn kubernetes_turns_networks_and_reach_into_network_policies() -> Outcome {
    let spec = example("segmented")?;
    let files = generate(&spec, Target::Kubernetes)?;
    let docs = yaml_documents(file(&files, ".isoloom/kubernetes/environment.yaml")?);
    // Deny by default, each network's machines together, then the reach rules (with ports).
    let policies = manifest_names(&docs, "NetworkPolicy");
    // Reach policies carry their index so a dash in a network name can't collide two of them.
    for name in ["isoloom-default-deny", "net-back", "reach-0-access-front", "reach-1-front-back"] {
        assert!(policies.contains(&name.to_owned()), "{name}: {policies:?}");
    }
    let reach = manifest(&docs, "NetworkPolicy", "reach-1-front-back");
    assert_eq!(yaml_str(reach, "spec.podSelector.matchLabels.`net.isoloom.com/back`"), "member");
    assert_eq!(
        yaml_str(reach, "spec.ingress.0.from.0.podSelector.matchLabels.`net.isoloom.com/front`"),
        "member"
    );
    assert_eq!(yaml_str(reach, "spec.ingress.0.ports.0.port"), "6379");
    // Names resolve as everywhere else: a hostname and a Service per serving machine.
    assert_eq!(yaml_str(manifest(&docs, "Deployment", "web"), "spec.template.spec.hostname"), "web");
    assert_eq!(yaml_str(manifest(&docs, "Service", "web"), "spec.selector.`isoloom.com/machine`"), "web");
    // A volume is a claim.
    assert_ne!(manifest_names(&docs, "PersistentVolumeClaim").len(), 0);
    // Checks: a Job per position, the user's with the access network's label (its policies apply),
    // the others carrying their machine's networks; each runs its own runner script.
    let jobs = yaml_documents(file(&files, ".isoloom/kubernetes/checks/job.yaml")?);
    let user = manifest(&jobs, "Job", "isoloom-check");
    assert_eq!(yaml_str(user, "spec.template.metadata.labels.`net.isoloom.com/access`"), "member");
    let web = manifest(&jobs, "Job", "isoloom-check-web");
    assert!(yaml_strs(web, "spec.template.spec.containers.0.command").contains(&"/isoloom/run/web.sh".to_owned()));
    let k = yaml_documents(file(&files, ".isoloom/kubernetes/checks/kustomization.yaml")?);
    let runners = yaml_strs(&k[0], "configMapGenerator.0.files");
    assert!(runners.contains(&"user.sh".to_owned()) && runners.contains(&"web.sh".to_owned()), "{runners:?}");
    // Machines are reached by name (Kubernetes picks the addresses).
    let web_sh = file(&files, ".isoloom/kubernetes/checks/web.sh")?;
    assert!(web_sh.contains("_tcp 'cache' 6379"), "{web_sh}");
    Ok(())
}

#[test]
fn kubernetes_seeds_a_volume_from_the_image_before_the_init_jobs() -> Outcome {
    // `volumes: { data: /chroot }` over an image path: on Docker the named volume starts with the
    // image's files; the claim does too, from an init container of the machine's image.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  chal:\n    networks: { app: 10 }\n    volumes: { data: /chroot }\n    app: { image: \"gcr.io/distroless/static\" }\n    setup: [secret.sh]\n",
    )?;
    let (docs, _) = environment(&spec)?;
    let pod = yaml_at(manifest(&docs, "Deployment", "chal"), "spec.template.spec").ok_or("no pod")?;
    // An init container: done before the machine and its init jobs (a container of the pod) start.
    let inits: Vec<String> = yaml_at(pod, "initContainers")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .map(|c| yaml_str(c, "name"))
        .collect();
    // The busybox it runs is copied in first, even though the machine has no services to probe.
    assert_eq!(inits.first().map(String::as_str), Some("isoloom-probe"), "{inits:?}");
    assert!(inits.contains(&"seed-data".to_owned()), "{inits:?}");
    let probe_volume = yaml_at(named(pod, "volumes", "isoloom-probe"), "emptyDir").and_then(Value::as_mapping);
    assert!(probe_volume.is_some_and(serde_yaml_ng::Mapping::is_empty), "{probe_volume:?}");
    let seed = named(pod, "initContainers", "seed-data");
    // The machine's image, run by Isoloom's busybox (no shell needed from the image), as root,
    // the claim at a path of its own, copied into only while empty (a `lost+found` aside).
    assert_eq!(yaml_str(seed, "image"), "gcr.io/distroless/static");
    assert_eq!(yaml_strs(seed, "command")[..2], ["/.isoloom-probe/busybox", "sh"]);
    assert_eq!(yaml_str(seed, "securityContext.runAsUser"), "0");
    assert_eq!(yaml_str(named(seed, "volumeMounts", "vol-data"), "mountPath"), "/isoloom-seed");
    // Its script, as the kubelet hands it to the shell: every `$` is written doubled so the
    // kubelet expands nothing, and it reads `$$` back as `$`.
    let script = yaml_str(seed, "command.3").replace("$$", "$");
    assert!(script.contains("[ -d '/chroot' ] || exit 0"), "{script}");
    assert!(script.contains("lost+found"), "{script}");
    assert!(script.contains("$b cp -a '/chroot/.' \"$d/\""), "{script}");
    // A marker stands while copying: a failed copy is cleared and done again, not taken as seeded.
    assert!(script.contains(": > \"$m\" && $b cp -a") && script.ends_with("&& $b rm -f \"$m\""), "{script}");
    assert!(script.contains("[ -e \"$m\" ] || exit 0; $b rm -rf \"$f\""), "{script}");
    Ok(())
}

#[test]
fn kubernetes_publishes_and_keeps_offline_machines_inside() -> Outcome {
    let spec = example("hello-stack")?;
    let (docs, text) = environment(&spec)?;
    // A published port is a cluster-internal Service, reached through `kubectl port-forward`:
    // nothing opens a public load balancer.
    let services = manifest_names(&docs, "Service");
    assert!(
        services.iter().all(|s| yaml_at(manifest(&docs, "Service", s), "spec.type").is_none()),
        "{services:?}"
    );
    assert_eq!(yaml_str(manifest(&docs, "Service", "isoloom-web"), "spec.ports.0.port"), "8080");
    assert!(
        text.contains("#   kubectl -n isoloom-hello-stack port-forward svc/isoloom-web 8080:8080\n"),
        "{text}"
    );
    let notes = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/chart/templates/NOTES.txt")?;
    assert!(
        notes.contains("kubectl -n {{ .Release.Namespace }} port-forward svc/isoloom-web 8080:8080"),
        "{notes}"
    );
    // The machines and the checks that stand on the offline networks are offline too: egress
    // to the environment and the cluster's DNS only.
    let policies = manifest_names(&docs, "NetworkPolicy");
    for offline in ["offline-cache", "offline-web", "offline-isoloom-check"] {
        let policy = manifest(&docs, "NetworkPolicy", offline);
        assert_eq!(yaml_strs(policy, "spec.policyTypes"), ["Egress"], "{offline}: {policies:?}");
    }
    Ok(())
}

#[test]
fn kubernetes_pods_run_under_the_baseline_pod_security_level() -> Outcome {
    let (docs, _) = environment(&example("segmented")?)?;
    // The namespace enforces `baseline`; each pod the runtime's seccomp filter.
    let ns = manifest(&docs, "Namespace", "isoloom-segmented");
    assert_eq!(yaml_str(ns, "metadata.labels.`pod-security.kubernetes.io/enforce`"), "baseline");
    let deployment = manifest(&docs, "Deployment", "web");
    assert_eq!(yaml_str(deployment, "spec.template.spec.securityContext.seccompProfile.type"), "RuntimeDefault");
    // A machine's container: no escalation, its capabilities dropped but what a root process of a
    // stock image needs, never raw sockets.
    let web = container(&docs, "web");
    assert_eq!(yaml_str(web, "securityContext.allowPrivilegeEscalation"), "false");
    assert_eq!(yaml_strs(web, "securityContext.capabilities.drop"), ["ALL"]);
    let added = yaml_strs(web, "securityContext.capabilities.add");
    assert!(added.contains(&"SETUID".to_owned()) && !added.contains(&"NET_RAW".to_owned()), "{added:?}");
    assert!(yaml_at(web, "securityContext.privileged").is_none());
    // A machine that doesn't size itself is limited to a container's default size and reserves a
    // small floor.
    let size = |c: &Value| ["requests.cpu", "requests.memory", "limits.cpu", "limits.memory"].map(|k| yaml_str(c, &format!("resources.{k}")));
    assert_eq!(size(web), ["100m", "128Mi", "1", "512Mi"]);
    // Isoloom's busybox helpers run as nobody, with no capability.
    let probe = named(deployment, "spec.template.spec.initContainers", "isoloom-probe");
    assert_eq!(yaml_str(probe, "securityContext.runAsUser"), "65534");
    assert_eq!(yaml_str(probe, "securityContext.runAsNonRoot"), "true");
    // One that does is limited to its size (1 CPU, 1 GiB), all of it reserved.
    assert_eq!(size(container(&docs, "cache")), ["1", "1024Mi", "1", "1024Mi"]);
    Ok(())
}

#[test]
fn a_privileged_machine_stays_privileged_and_its_namespace_allows_it() -> Outcome {
    let spec = allowed(lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  box: { networks: { lan: 10 }, privileged: true, app: { image: alpine } }\n  web: { networks: { lan: 11 }, app: { image: nginx } }\n",
    )?);
    let (docs, _) = environment(&spec)?;
    let ns = manifest(&docs, "Namespace", "isoloom-t");
    assert_eq!(yaml_str(ns, "metadata.labels.`pod-security.kubernetes.io/enforce`"), "privileged");
    let privileged = container(&docs, "box");
    assert_eq!(yaml_str(privileged, "securityContext.privileged"), "true");
    assert!(yaml_at(privileged, "securityContext.capabilities.drop").is_none());
    assert!(yaml_at(manifest(&docs, "Deployment", "box"), "spec.template.spec.securityContext.seccompProfile").is_none());
    let other = container(&docs, "web");
    assert_eq!(yaml_str(other, "securityContext.allowPrivilegeEscalation"), "false");
    assert!(yaml_at(other, "securityContext.privileged").is_none());
    Ok(())
}
