//! A machine's `environment`, `sysctls` and `ulimits`: from the container runtime on the
//! container targets, from the VM's own configuration (one set-up script) on the VM targets.

use isoloom_spec::syntax::Tpm;
use isoloom_spec::{Spec, Target, parse};
use isoloom_targets::{Lab, refusal};
use isoloom_test_support::{manifest, named, yaml_at, yaml_documents, yaml_str};

use crate::testing::{Outcome, cloud_config, define, generated, provisioners, set_up, source, strings, vagrantfile};

/// The tuned-web example.
fn tuned() -> Outcome<Spec> {
    source("tuned-web")
}

/// The example without its TPM: `hardware.tpm: required` is VM only, so this is the machine
/// the container targets can run.
fn tuned_container() -> Outcome<Spec> {
    let mut spec = tuned()?;
    spec.machines.get_mut("web").ok_or("tuned-web has no web")?.hardware.tpm = Tpm::Auto;
    Ok(spec)
}

/// The file of `spec` on `t` whose path ends with `suffix`.
fn file(spec: &Spec, t: Target, suffix: &str) -> Outcome<String> {
    generated(&Lab::new(spec.clone()), t, suffix)
}

#[test]
fn compose_sets_them_on_the_container() -> Outcome {
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&file(&tuned_container()?, Target::Docker, "compose.yml")?)?;
    let web = &doc["services"]["web"];
    assert_eq!(web["environment"]["APP_MODE"].as_str(), Some("production"));
    assert_eq!(web["sysctls"]["net.core.somaxconn"].as_str(), Some("4096"));
    assert_eq!(web["ulimits"]["nofile"]["soft"].as_u64(), Some(8192));
    assert_eq!(web["ulimits"]["nofile"]["hard"].as_u64(), Some(16384));
    Ok(())
}

#[test]
fn every_vm_target_runs_the_same_settings_before_the_steps() -> Outcome {
    let spec = tuned()?;
    let vf = vagrantfile(&Lab::new(spec.clone()))?;
    let web = define(&vf, "web");
    let steps = provisioners(web);
    let at = |name: &str| steps.iter().position(|s| s == name).ok_or_else(|| format!("no `{name}` step in {steps:?}"));
    assert!(at("settings")? < at("provision/web.sh")?, "{steps:?}");
    let settings = web.statements("m.vm.provision \"shell\", name: \"settings\"").concat();
    for needle in [
        "APP_MODE=production",
        "net.core.somaxconn = 4096",
        "soft nofile 8192",
        "DefaultLimitNOFILE=8192:16384",
        "daemon-reexec",
    ] {
        assert!(settings.contains(needle), "Vagrant: {needle}: {settings}");
    }
    let pve = strings(&cloud_config(&file(&spec, Target::Proxmox, "proxmox/main.tf")?, "web")?, "runcmd");
    assert!(pve.iter().any(|c| c.contains("net.core.somaxconn = 4096")), "Proxmox runcmd: {pve:?}");
    // Linode gives no TPM, which the machine asks for: its module is left out.
    for cloud in ["aws", "azure", "gcp", "oci"] {
        let script = set_up(&file(&spec, Target::CloudVm, &format!("cloud-vm/{cloud}/main.tf"))?, "web")?;
        assert!(
            script.contains("{ sh -c 'set -e") && script.contains("APP_MODE=production"),
            "{cloud}: {script}"
        );
    }
    Ok(())
}

#[test]
fn kubernetes_refuses_ulimits_and_sets_the_rest() -> Outcome {
    let why = refusal(&Lab::new(tuned_container()?), Target::Kubernetes)?.ok_or("ulimits refused")?;
    assert!(why.contains("process limits"), "{why}");
    let mut spec = tuned_container()?;
    spec.machines["web"].ulimits.clear();
    let docs = yaml_documents(&file(&spec, Target::Kubernetes, "kubernetes/environment.yaml")?);
    let pod = yaml_at(manifest(&docs, "Deployment", "web"), "spec.template.spec").ok_or("no pod")?;
    assert_eq!(yaml_str(named(named(pod, "containers", "web"), "env", "APP_MODE"), "value"), "production");
    assert_eq!(yaml_str(named(pod, "securityContext.sysctls", "net.core.somaxconn"), "value"), "4096");
    Ok(())
}

#[test]
fn the_fields_are_validated() {
    let spec = parse(
        "version: 1
name: bad
networks: { app: { cidr: 10.83.0.0/24 } }
machines:
  web:
    networks: { app: 10 }
    environment: { 1BAD: x }
    sysctls: { vm.swappiness: 10, Bad: 1 }
    ulimits: { nofiles: 1, nproc: { soft: 9, hard: 1 } }
    app: { image: nginx }
  dc:
    os: windows-server@2019
    networks: { app: 11 }
    sysctls: { net.core.somaxconn: 1 }
    app: { steps: [{ run: dc.ps1 }] }
",
    )
    .unwrap();
    let e = parse("version: 1\nname: x\nnetworks: {}\nmachines: { a: { environment: { K: [1] } } }\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("expected a text, a number, a boolean, or `{ input: NAME }`"), "{e}");
    let problems: Vec<String> = Lab::new(spec).problems().iter().map(isoloom_spec::syntax::Problem::line).collect();
    let has = |at: &str, what: &str| problems.iter().any(|p| p.starts_with(at) && p.contains(what));
    assert!(has("machines.web.environment.1BAD", "variable's name"), "{problems:#?}");
    assert!(has("machines.web.sysctls.vm.swappiness", "namespaced"), "{problems:#?}");
    assert!(has("machines.web.sysctls.Bad", "kernel parameter"), "{problems:#?}");
    assert!(has("machines.web.ulimits.nofiles", "one of"), "{problems:#?}");
    assert!(has("machines.web.ulimits.nproc", "soft limit"), "{problems:#?}");
    assert!(has("machines.dc.sysctls", "Linux only"), "{problems:#?}");
}
