//! `tools:`: observers beside the environment, outside its contract.

use std::path::Path;

use isoloom_core::{Target, checks, generate, load, parse, resolved, validate};

#[test]
fn the_shell_tool_sits_on_every_network_at_a_reserved_address() {
    let spec = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/slow-link")).unwrap();
    assert_eq!(validate(&spec), vec![]);
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
    assert!(checks::plan(&spec).iter().all(|c| !c.name.contains("shell")));
    assert_eq!(isoloom_core::totals(&spec).machines, 2);
    let r = resolved::resolve(&spec);
    assert_eq!(resolved::lookup(&r, "tools.shell.addresses.far").unwrap(), "10.75.2.252");
}

#[test]
fn tools_are_checked() {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 5 }, docker: { image: x } }\n";
    let p = |tools: &str| -> Vec<String> {
        validate(&parse(&format!("{base}tools:\n{tools}")).unwrap())
            .into_iter()
            .map(|p| p.to_string())
            .collect()
    };
    assert_eq!(p("  shell: {}\n"), Vec::<String>::new());
    assert!(p("  viewer: {}\n")[0].contains("give a container image"));
    assert!(p("  shell: { image: x }\n")[0].contains("recipe"));
    assert!(p("  viewer: { image: x, publish: 9000 }\n")[0].contains("which `port`"));
    assert!(p("  a: { image: x }\n")[0].contains("already a machine's"));
    // The reserved addresses stay free of machines.
    let taken = validate(&parse("version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  a: { networks: { lab: 252 }, docker: { image: x } }\ntools:\n  shell: {}\n").unwrap());
    assert!(
        taken
            .iter()
            .any(|p| p.at == "machines.a.networks.lab" && p.message.contains("reserved for the tools")),
        "{taken:?}"
    );
    // A custom image tool with a published UI.
    let spec = parse(&format!("{base}tools:\n  viewer: {{ image: ghcr.io/x/viewer, port: 8080, publish: 9000 }}\n")).unwrap();
    let compose = &generate(&spec, Target::Docker).unwrap()[0].contents;
    assert!(compose.contains("isoloom-tool-viewer:") && compose.contains(":9000:8080"), "{compose}");
}
