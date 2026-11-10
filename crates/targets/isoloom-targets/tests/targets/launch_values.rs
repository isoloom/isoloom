//! The values given at launch on their way into a Vagrant VM: every provisioner's environment
//! carries them in base64, which its script decodes, since Vagrant writes that environment into a
//! double-quoted shell string where a `$(...)` or a backquote in a value would run, as root.

use isoloom_spec::Target;

use crate::testing::{Outcome, generated, lab};

/// A container reading an input, checked by a script: docker-vm's environment and checks
/// provisioners, and hybrid's containers, read it.
const WITH_INPUT: &str = "
version: 1
name: launched
inputs: [TOKEN]
networks: { lab: { cidr: 10.32.0.0/24 } }
machines:
  web:
    networks: { lab: 10 }
    environment: { TOKEN: { input: TOKEN } }
    app: { image: nginx }
checks:
  - check.sh
";

/// [`WITH_INPUT`] with a Windows VM beside its container: a lab for `hybrid`.
fn with_a_vm() -> String {
    WITH_INPUT.replace(
        "checks:",
        "  files01:\n    os: windows-server@2019\n    networks: { lab: 11 }\n    app: { steps: [{ run: p.ps1 }] }\nchecks:",
    )
}

/// The provisioner `name` of a Vagrantfile, from its name up to the next provisioner or the end of
/// its block.
fn provisioner<'a>(vagrantfile: &'a str, name: &str) -> &'a str {
    let rest = vagrantfile.split_once(&format!("name: \"{name}\"")).map_or("", |(_, rest)| rest);
    let end = ["\n  config.vm.provision", "\n    m.vm.provision", "\nend", "\n  end"]
        .iter()
        .filter_map(|e| rest.find(e))
        .min()
        .unwrap_or(rest.len());
    rest.get(..end).unwrap_or(rest)
}

#[test]
fn docker_vm_hands_its_provisioners_the_values_in_base64_and_decodes_them_first() -> Outcome {
    let v = generated(&lab(WITH_INPUT)?, Target::DockerVm, "docker-vm/Vagrantfile")?;
    assert!(!v.contains("env: INPUTS") && !v.contains("INPUTS.merge"), "{v}");
    assert!(v.contains("def isoloom_b64(value)"), "{v}");
    for name in ["environment", "checks"] {
        let call = provisioner(&v, name);
        assert!(call.contains("env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }"), "{name}: {call}");
        let script = call.split_once("inline: ").map(|(_, s)| s).unwrap_or_default();
        assert!(
            script.starts_with("\"TOKEN=\\\"$(printenv TOKEN | base64 -d; printf x)\\\""),
            "{name}: {script}"
        );
    }
    Ok(())
}

#[test]
fn hybrids_containers_get_the_values_in_base64_and_decode_them_first() -> Outcome {
    let v = generated(&lab(&with_a_vm())?, Target::Hybrid, "hybrid/Vagrantfile")?;
    let call = provisioner(&v, "containers");
    assert!(call.contains("env: { \"TOKEN\" => isoloom_b64(INPUTS[\"TOKEN\"]) }"), "{call}");
    assert!(call.contains("inline: \"TOKEN=\\\"$(printenv TOKEN | base64 -d; printf x)\\\""), "{call}");
    Ok(())
}
