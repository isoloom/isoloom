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
    // `volumes: { ctf: /chroot }` over an image path: on Docker the named volume starts with the
    // image's files; the claim does too, from an init container of the machine's image.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  chal:\n    networks: { app: 10 }\n    volumes: { ctf: /chroot }\n    app: { image: \"gcr.io/distroless/static\" }\n    setup: [flag.sh]\n",
    )?;
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    let seed = env.find("- name: seed-ctf").expect("a seed init container");
    let init = env.find("initContainers:").expect("init containers");
    let containers = env.find("containers:\n      - name: chal").expect("the machine");
    // An init container: done before the machine and its init jobs (a container of the pod) start.
    assert!(init < seed && seed < containers, "{env}");
    let block = between(&env, "- name: seed-ctf", "containers:\n      - name: chal")?;
    // The machine's image, run by Isoloom's busybox (no shell needed from the image), as root,
    // the claim at a path of its own, copied into only while empty (a `lost+found` aside).
    assert!(block.contains("image: gcr.io/distroless/static"), "{block}");
    assert!(block.contains("- /.isoloom-probe/busybox\n        - sh"), "{block}");
    assert!(block.contains("runAsUser: 0"), "{block}");
    assert!(block.contains("name: vol-ctf\n          mountPath: /isoloom-seed"), "{block}");
    // Its script, as Kubernetes reads it.
    let docs: Vec<serde_yaml_ng::Value> = env.split("\n---\n").filter_map(|d| serde_yaml_ng::from_str(d).ok()).collect();
    let deploy = docs.iter().find(|d| d["kind"] == "Deployment").expect("the machine's Deployment");
    let inits = deploy["spec"]["template"]["spec"]["initContainers"].as_sequence().unwrap();
    let seed_c = inits.iter().find(|c| c["name"] == "seed-ctf").unwrap();
    let script = seed_c["command"][3].as_str().unwrap();
    assert!(script.contains("[ -d '/chroot' ] || exit 0"), "{script}");
    assert!(script.contains("lost+found"), "{script}");
    assert!(script.contains("$b cp -a '/chroot/.' \"$d/\""), "{script}");
    // A marker stands while copying: a failed copy is cleared and done again, not taken as seeded.
    assert!(script.contains(": > \"$m\" && $b cp -a") && script.ends_with("&& $b rm -f \"$m\""), "{script}");
    assert!(script.contains("[ -e \"$m\" ] || exit 0; $b rm -rf \"$f\""), "{script}");
    // The busybox it runs is copied in first, even though the machine has no services to probe.
    assert!(
        between(&env, "initContainers:", "- name: seed-ctf")?.contains("- name: isoloom-probe\n        image: busybox"),
        "{env}"
    );
    assert!(env.contains("- name: isoloom-probe\n        emptyDir: {}"), "{env}");
    Ok(())
}

#[test]
fn kubernetes_publishes_and_keeps_offline_machines_inside() -> Outcome {
    let spec = example("hello-stack")?;
    let env = generated(&spec, Target::Kubernetes, ".isoloom/kubernetes/environment.yaml")?;
    assert!(env.contains("type: LoadBalancer") && env.contains("port: 8080"), "{env}");
    assert!(env.contains("name: offline-"));
    // The checks stand on those offline networks: offline too.
    assert!(env.contains("name: offline-isoloom-check"));
    assert!(env.contains("policyTypes:\n    - Egress") || env.contains("policyTypes:\n  - Egress"));
    Ok(())
}
