//! `tools:`: observers beside the environment, outside its contract.

use isoloom_plan::checks;
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, generate, resolved};

use crate::testing::{Outcome, generated, lab};

/// The problems of the spec `yaml`, each as `path: reason`.
fn problems(yaml: &str) -> Outcome<Vec<String>> {
    Ok(crate::testing::problems(&lab(yaml)?))
}

#[test]
fn the_shell_tool_sits_on_every_network_at_a_reserved_address() -> Outcome {
    let spec = Lab::new(load(&crate::testing::examples().join("slow-link")).unwrap());
    assert_eq!(spec.problems(), vec![]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-tool-shell:") && compose.contains("nicolaka/netshoot"), "{compose}");
    assert!(
        compose.contains("ipv4_address: 10.75.1.252") && compose.contains("ipv4_address: 10.75.2.252"),
        "{compose}"
    );
    let vf = generated(&spec, Target::Vagrant, "Vagrantfile")?;
    assert!(
        vf.contains("config.vm.define \"isoloom-tool-shell\"") && vf.contains("tcpdump nmap curl dnsutils"),
        "{vf}"
    );
    // Not a machine: no derived check names it, resources leave it out, the snapshot lists it apart.
    assert!(checks::plan(&spec.spec()).iter().all(|c| !c.name.contains("shell")));
    assert_eq!(isoloom_spec::totals(&spec.spec()).machines, 2);
    let r = resolved::resolve(&spec);
    assert_eq!(resolved::lookup(&r, "tools.shell.addresses.far").unwrap(), "10.75.2.252");
    Ok(())
}

#[test]
fn tools_are_checked() -> Outcome {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x } }\n";
    let p = |tools: &str| -> Outcome<Vec<String>> { problems(&format!("{base}tools:\n{tools}")) };
    assert_eq!(p("  shell: {}\n")?, Vec::<String>::new());
    assert!(p("  viewer: {}\n")?[0].contains("give a container image"));
    assert!(p("  shell: { image: x }\n")?[0].contains("recipe"));
    assert!(p("  viewer: { image: x, publish: 9000 }\n")?[0].contains("which `port`"));
    assert!(p("  a: { image: x }\n")?[0].contains("already a machine's"));
    // The reserved addresses stay free of machines.
    let taken = problems(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 252 }, app: { image: x } }\ntools:\n  shell: {}\n",
    )?;
    assert!(
        taken
            .iter()
            .any(|t| t.starts_with("machines.a.networks.lab: ") && t.contains("reserved for the tools")),
        "{taken:?}"
    );
    // A custom image tool with a published UI.
    let spec = Lab::new(parse(&format!("{base}tools:\n  viewer: {{ image: ghcr.io/x/viewer, port: 8080, publish: 9000 }}\n")).unwrap());
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-tool-viewer:") && compose.contains(":9000:8080"), "{compose}");
    Ok(())
}
