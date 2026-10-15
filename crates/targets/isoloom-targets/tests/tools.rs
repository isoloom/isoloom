//! `tools:`: observers beside the environment, outside its contract.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::checks;
use isoloom_spec::{Target, load, parse};
use isoloom_targets::{Lab, generate, resolved};

/// The lab's problems, each as `path: reason`.
fn problems(yaml: &str) -> Vec<String> {
    Lab::new(parse(yaml).unwrap())
        .problems()
        .iter()
        .map(isoloom_spec::syntax::Problem::line)
        .collect()
}

#[test]
fn the_shell_tool_sits_on_every_network_at_a_reserved_address() {
    let spec = Lab::new(load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/slow-link")).unwrap());
    assert_eq!(spec.problems(), vec![]);
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-tool-shell:") && compose.contains("nicolaka/netshoot"), "{compose}");
    assert!(
        compose.contains("ipv4_address: 10.75.1.252") && compose.contains("ipv4_address: 10.75.2.252"),
        "{compose}"
    );
    let vf = generate(&spec, Target::Vagrant)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with("Vagrantfile"))
        .unwrap()
        .contents;
    assert!(
        vf.contains("config.vm.define \"isoloom-tool-shell\"") && vf.contains("tcpdump nmap curl dnsutils"),
        "{vf}"
    );
    // Not a machine: no derived check names it, resources leave it out, the snapshot lists it apart.
    assert!(checks::plan(&spec.spec()).iter().all(|c| !c.name.contains("shell")));
    assert_eq!(isoloom_spec::totals(&spec.spec()).machines, 2);
    let r = resolved::resolve(&spec);
    assert_eq!(resolved::lookup(&r, "tools.shell.addresses.far").unwrap(), "10.75.2.252");
}

#[test]
fn tools_are_checked() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, app: { image: x } }\n";
    let p = |tools: &str| -> Vec<String> { problems(&format!("{base}tools:\n{tools}")) };
    assert_eq!(p("  shell: {}\n"), Vec::<String>::new());
    assert!(p("  viewer: {}\n")[0].contains("give a container image"));
    assert!(p("  shell: { image: x }\n")[0].contains("recipe"));
    assert!(p("  viewer: { image: x, publish: 9000 }\n")[0].contains("which `port`"));
    assert!(p("  a: { image: x }\n")[0].contains("already a machine's"));
    // The reserved addresses stay free of machines.
    let taken = problems(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 252 }, app: { image: x } }\ntools:\n  shell: {}\n",
    );
    assert!(
        taken
            .iter()
            .any(|p| p.starts_with("machines.a.networks.lab: ") && p.contains("reserved for the tools")),
        "{taken:?}"
    );
    // A custom image tool with a published UI.
    let spec = Lab::new(parse(&format!("{base}tools:\n  viewer: {{ image: ghcr.io/x/viewer, port: 8080, publish: 9000 }}\n")).unwrap());
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-tool-viewer:") && compose.contains(":9000:8080"), "{compose}");
}
