//! The `kubernetes` target: network policies, volumes seeded from the image, published and offline
//! machines.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, between, example, file, generated, lab};

#[test]
fn kubernetes_turns_networks_and_reach_into_network_policies() -> Outcome {
    let spec = example("segmented")?;
    let files = generate(&spec, Target::Kubernetes).unwrap();
    let env = file(&files, ".isoloom/kubernetes/environment.yaml")?;
    // Deny by default, each network's machines together, then the reach rules (with ports).
    assert!(env.contains("name: isoloom-default-deny"));
    assert!(env.contains("name: net-back"));
    // Reach policies carry their index so a dash in a network name can't collide two of them.
    assert!(env.contains("name: reach-0-access-front"));
    assert!(env.contains("name: reach-1-front-back"));
    assert!(env.contains("port: 6379"));
    // Names resolve as everywhere else: a hostname and a Service per serving machine.
    assert!(env.contains("hostname: web") && env.contains("kind: Service\nmetadata:\n  name: web"));
    // A volume is a claim.
    assert!(env.contains("kind: PersistentVolumeClaim"));
    // Checks: a Job per position, the user's with the access network's label (its policies apply),
    // the others carrying their machine's networks; each runs its own runner script.
    let job = file(&files, ".isoloom/kubernetes/checks/job.yaml")?;
    assert!(job.contains("net.isoloom.com/access: member"), "{job}");
    assert!(job.contains("name: isoloom-check-web") && job.contains("/isoloom/run/web.sh"), "{job}");
    let k = file(&files, ".isoloom/kubernetes/checks/kustomization.yaml")?;
    assert!(k.contains("- user.sh") && k.contains("- web.sh"), "{k}");
    // Machines are reached by name (Kubernetes picks the addresses).
    let web = file(&files, ".isoloom/kubernetes/checks/web.sh")?;
    assert!(web.contains("_tcp 'cache' 6379"), "{web}");
    Ok(())
}

#[test]
fn kubernetes_seeds_a_volume_from_the_image_before_the_init_jobs() -> Outcome {
    // `volumes: { data: /chroot }` over an image path: on Docker the named volume starts with the
    // image's files; the claim does too, from an init container of the machine's image.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  chal:\n    networks: { app: 10 }\n    volumes: { data: /chroot }\n    app: { image: \"gcr.io/distroless/static\" }\n    setup: [secret.sh]\n",
    )?;
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    let seed = env.find("- name: seed-data").expect("a seed init container");
    let init = env.find("initContainers:").expect("init containers");
    let containers = env.find("containers:\n      - name: chal").expect("the machine");
    // An init container: done before the machine and its init jobs (a container of the pod) start.
    assert!(init < seed && seed < containers, "{env}");
    let block = between(&env, "- name: seed-data", "containers:\n      - name: chal")?;
    // The machine's image, run by Isoloom's busybox (no shell needed from the image), as root,
    // the claim at a path of its own, copied into only while empty (a `lost+found` aside).
    assert!(block.contains("image: gcr.io/distroless/static"), "{block}");
    assert!(block.contains("- /.isoloom-probe/busybox\n        - sh"), "{block}");
    assert!(block.contains("runAsUser: 0"), "{block}");
    assert!(block.contains("name: vol-data\n          mountPath: /isoloom-seed"), "{block}");
    // Its script, as Kubernetes reads it.
    let docs: Vec<serde_yaml_ng::Value> = env.split("\n---\n").filter_map(|d| serde_yaml_ng::from_str(d).ok()).collect();
    let deploy = docs.iter().find(|d| d["kind"] == "Deployment").expect("the machine's Deployment");
    let inits = deploy["spec"]["template"]["spec"]["initContainers"].as_sequence().unwrap();
    let seed_c = inits.iter().find(|c| c["name"] == "seed-data").unwrap();
    let script = seed_c["command"][3].as_str().unwrap();
    assert!(script.contains("[ -d '/chroot' ] || exit 0"), "{script}");
    assert!(script.contains("lost+found"), "{script}");
    assert!(script.contains("$b cp -a '/chroot/.' \"$d/\""), "{script}");
    // A marker stands while copying: a failed copy is cleared and done again, not taken as seeded.
    assert!(script.contains(": > \"$m\" && $b cp -a") && script.ends_with("&& $b rm -f \"$m\""), "{script}");
    assert!(script.contains("[ -e \"$m\" ] || exit 0; $b rm -rf \"$f\""), "{script}");
    // The busybox it runs is copied in first, even though the machine has no services to probe.
    assert!(
        between(&env, "initContainers:", "- name: seed-data")?.contains("- name: isoloom-probe\n        image: busybox"),
        "{env}"
    );
    assert!(env.contains("- name: isoloom-probe\n        emptyDir: {}"), "{env}");
    Ok(())
}

#[test]
fn kubernetes_publishes_and_keeps_offline_machines_inside() -> Outcome {
    let spec = example("hello-stack")?;
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    // A published port is a cluster-internal Service, reached through `kubectl port-forward`:
    // nothing opens a public load balancer.
    assert!(
        !env.contains("LoadBalancer") && env.contains("name: web-published") && env.contains("port: 8080"),
        "{env}"
    );
    assert!(
        env.contains("#   kubectl -n isoloom-hello-stack port-forward svc/web-published 8080:8080\n"),
        "{env}"
    );
    let notes = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/chart/templates/NOTES.txt")?;
    assert!(
        notes.contains("kubectl -n {{ .Release.Namespace }} port-forward svc/web-published 8080:8080"),
        "{notes}"
    );
    assert!(env.contains("name: offline-"));
    // The checks stand on those offline networks: offline too.
    assert!(env.contains("name: offline-isoloom-check"));
    assert!(env.contains("policyTypes:\n    - Egress") || env.contains("policyTypes:\n  - Egress"));
    Ok(())
}

#[test]
fn kubernetes_pods_run_under_the_baseline_pod_security_level() -> Outcome {
    let env = generated(&example("segmented")?, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    // The namespace enforces `baseline`; each pod the runtime's seccomp filter.
    assert!(env.contains("pod-security.kubernetes.io/enforce: baseline"), "{env}");
    assert!(
        env.contains("securityContext:\n        seccompProfile:\n          type: RuntimeDefault"),
        "{env}"
    );
    // A machine's container: no escalation, its capabilities dropped but what a root process of a
    // stock image needs, never raw sockets.
    let web = between(&env, "containers:\n      - name: web\n", "volumes:")?;
    assert!(
        web.contains("allowPrivilegeEscalation: false") && web.contains("drop:\n            - ALL"),
        "{web}"
    );
    assert!(web.contains("- SETUID") && !web.contains("NET_RAW") && !web.contains("privileged"), "{web}");
    // A machine that doesn't size itself is limited to a container's default size and reserves a
    // small floor.
    assert!(
        web.contains("requests:\n            cpu: 100m\n            memory: 128Mi\n          limits:\n            cpu: '1'\n            memory: 512Mi"),
        "{web}"
    );
    // Isoloom's busybox helpers run as nobody, with no capability.
    let probe = between(&env, "- name: isoloom-probe\n", "volumeMounts")?;
    assert!(probe.contains("runAsUser: 65534") && probe.contains("runAsNonRoot: true"), "{probe}");
    // One that does is limited to its size (1 CPU, 1 GiB), all of it reserved.
    let cache = between(&env, "containers:\n      - name: cache\n", "volumes:")?;
    assert!(
        cache.contains("requests:\n            cpu: '1'\n            memory: 1024Mi\n          limits:\n            cpu: '1'\n            memory: 1024Mi"),
        "{cache}"
    );
    Ok(())
}

#[test]
fn a_privileged_machine_stays_privileged_and_its_namespace_allows_it() -> Outcome {
    let spec = lab(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  box: { networks: { lan: 10 }, privileged: true, app: { image: alpine } }\n  web: { networks: { lan: 11 }, app: { image: nginx } }\n",
    )?;
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    assert!(env.contains("pod-security.kubernetes.io/enforce: privileged"), "{env}");
    let privileged = between(&env, "name: box\n", "---")?;
    assert!(
        privileged.contains("privileged: true") && !privileged.contains("seccompProfile") && !privileged.contains("drop:"),
        "{privileged}"
    );
    let other = between(&env, "containers:\n      - name: web\n", "---")?;
    assert!(
        other.contains("allowPrivilegeEscalation: false") && !other.contains("privileged: true"),
        "{other}"
    );
    Ok(())
}
