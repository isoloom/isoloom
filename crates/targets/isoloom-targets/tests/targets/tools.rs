//! `tools:`: observers beside the environment, outside its contract.

use isoloom_plan::checks;
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, resolved};
use isoloom_test_support::{yaml, yaml_str, yaml_strs};

use crate::testing::{Outcome, define, generated, lab, vagrantfile};

/// The problems of the spec `yaml`, each as `path: reason`.
fn problems(yaml: &str) -> Outcome<Vec<String>> {
    Ok(crate::testing::problems(&lab(yaml)?))
}

#[test]
fn the_shell_tool_sits_on_every_network_at_a_reserved_address() -> Outcome {
    let spec = Lab::new(load(&crate::testing::examples().join("slow-link"))?);
    assert_eq!(spec.problems(), vec![]);
    let compose = yaml(&generated(&spec, Target::Docker, "docker/compose.yml")?);
    let shell = "services.isoloom-tool-shell";
    assert!(yaml_str(&compose, &format!("{shell}.image")).starts_with("nicolaka/netshoot:"));
    for (network, address) in [("branch", "10.75.1.252"), ("far", "10.75.2.252")] {
        assert_eq!(yaml_str(&compose, &format!("{shell}.networks.{network}.ipv4_address")), address);
    }
    let vf = vagrantfile(&spec)?;
    let vm = define(&vf, "isoloom-tool-shell").every_statement();
    assert!(vm.iter().any(|s| s.contains("tcpdump nmap curl dnsutils")), "{vm:?}");
    // Not a machine: no derived check names it, resources leave it out, the snapshot lists it apart.
    assert!(!checks::plan(&spec.spec()).iter().any(|c| c.name.contains("shell")));
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
    for (tool, at, says) in [
        ("  viewer: {}\n", "tools.viewer", "give a container image"),
        ("  shell: { image: x }\n", "tools.shell", "recipe"),
        ("  viewer: { image: x, publish: 9000 }\n", "tools.viewer", "which `port`"),
        ("  a: { image: x }\n", "tools.a", "already a machine's"),
    ] {
        let found = p(tool)?;
        assert!(
            matches!(found.as_slice(), [one] if one.starts_with(at) && one.contains(says)),
            "{tool}: {found:?}"
        );
    }
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
    let spec = Lab::new(parse(&format!(
        "{base}tools:\n  viewer: {{ image: ghcr.io/x/viewer, port: 8080, publish: 9000 }}\n"
    ))?);
    let compose = yaml(&generated(&spec, Target::Docker, "docker/compose.yml")?);
    let ports = yaml_strs(&compose, "services.isoloom-tool-viewer.ports");
    assert!(ports.iter().any(|port| port.ends_with(":9000:8080")), "{ports:?}");
    Ok(())
}
