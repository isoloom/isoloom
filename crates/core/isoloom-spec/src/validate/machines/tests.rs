use crate::testing::{lab, model, problems};

/// The problems of a lab whose machines are `machines` (YAML lines under `machines:`).
fn of(machines: &str) -> Vec<String> {
    problems(&lab(&format!("machines:\n{machines}")))
}

const WEB: &str = "  web: { networks: { lan: 10 }, docker: { image: nginx } }\n";

#[test]
fn a_lab_has_a_machine_and_one_access_machine_at_most() {
    assert_eq!(problems(&lab("")), ["machines: declare at least one machine"]);
    assert_eq!(
        problems("version: 1\nname: t\ncloud: { provider: aws, terraform: infra }\n"),
        Vec::<String>::new()
    );
    let one = "  a: { networks: { lan: 11 }, docker: { image: kali }, access: true }\n";
    assert_eq!(of(&format!("{one}{WEB}")), Vec::<String>::new());
    let two = "  b: { networks: { lan: 12 }, docker: { image: kali }, access: true }\n";
    assert_eq!(of(&format!("{one}{two}")), ["machines: only one machine can be the access machine (found 2)"]);
}

#[test]
fn a_network_appliance_runs_alone_on_docker() {
    let base = "networks: { lan: 10 }, docker: { image: vrnetlab/cisco_iol:17, appliance: cisco-iol";
    assert_eq!(of(&format!("  r1: {{ {base} }} }}\n")), Vec::<String>::new());
    assert_eq!(
        of(&format!("  r1: {{ {base} }}, vm: {{ os: debian@12 }} }}\n")),
        ["machines.r1.vm: a network appliance runs on the Docker targets only: leave `vm` out"]
    );
    assert_eq!(
        of(&format!("  r1: {{ {base} }}, access: true }}\n")),
        ["machines.r1.access: a network appliance can't be the access machine"]
    );
    let own_os = "machines.r1.docker: a network appliance runs its own OS: no `idle` or `init`";
    assert_eq!(of(&format!("  r1: {{ {base}, idle: true }} }}\n")), [own_os]);
    assert_eq!(of(&format!("  r1: {{ {base}, init: [a.sh] }} }}\n")), [own_os]);
    let project = "machines.r1.docker.config: a path in the project";
    assert_eq!(of(&format!("  r1: {{ {base}, config: /etc/r1.cfg }} }}\n")), [project]);
    assert_eq!(of(&format!("  r1: {{ {base}, config: ../r1.cfg }} }}\n")), [project]);
    assert_eq!(of(&format!("  r1: {{ {base}, config: configs/r1.cfg }} }}\n")), Vec::<String>::new());
}

#[test]
fn a_machines_inputs_are_declared() {
    let found = problems(&lab(
        "inputs: [FLAG]\nmachines:\n  web: { networks: { lan: 10 }, docker: { image: nginx }, inputs: [FLAG, TOKEN] }\n",
    ));
    assert_eq!(found, ["machines.web.inputs[1]: `TOKEN` isn't declared in the spec's `inputs`"]);
}

#[test]
fn a_machine_has_a_form_unless_the_runner_supplies_it() {
    assert_eq!(
        of("  web: { networks: { lan: 10 } }\n"),
        [
            "machines.web: give the machine a form: its `os`, plus an `app:` (or nothing more, the OS alone)",
            "machines: no target is possible: no shape is implemented by every machine (docker missing on: web; vm missing on: web)"
        ]
    );
    assert_eq!(of("  user: { networks: { lan: 10 }, access: true }\n"), Vec::<String>::new());
}

#[test]
fn a_machine_name_is_a_dns_label_short_enough_for_kubernetes() {
    let name = |n: &str| of(&format!("  {n}: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }} }}\n"));
    assert_eq!(name(&"a".repeat(53)), Vec::<String>::new());
    assert_eq!(
        name(&"a".repeat(54)),
        [format!(
            "machines.{}: at most 53 characters: it becomes the Kubernetes Service `<name>-published`",
            "a".repeat(54)
        )]
    );
    assert_eq!(name("Web"), ["machines.Web: machine names are DNS names: kebab-case, starting with a letter"]);
    assert_eq!(
        of("  web: { docker: { image: nginx } }\n"),
        ["machines.web.networks: attach the machine to at least one network"]
    );
}

#[test]
fn a_gateway_takes_its_networks_gateway_address() {
    let net = "  dmz: { cidr: 10.1.0.0/24, gateway: gw }\n";
    let at = |octet: u8| {
        problems(&lab(&format!(
            "{net}machines:\n  gw: {{ networks: {{ lan: 10, dmz: {octet} }}, docker: {{ image: router }} }}\n  web: {{ networks: {{ dmz: 20 }}, docker: {{ image: nginx }} }}\n"
        )))
    };
    assert_eq!(at(1), Vec::<String>::new());
    assert_eq!(
        at(5),
        ["machines.gw.networks.dmz: `gw` is this network's gateway, so it takes the gateway address: use 1"]
    );
}

#[test]
fn a_vm_is_sized_to_boot_and_a_container_to_run() {
    let vm = |r: &str| of(&format!("  web: {{ networks: {{ lan: 10 }}, vm: {{ os: debian@12 }}, resources: {r} }}\n"));
    assert_eq!(vm("{ cpus: 1, memory_mb: 256, disk_gb: 5 }"), Vec::<String>::new());
    let small_vm = "machines.web.resources: too small for a VM: at least 1 cpu, 256 MB and 5 GB";
    for r in ["{ cpus: 0 }", "{ memory_mb: 255 }", "{ disk_gb: 4 }"] {
        assert_eq!(vm(r), [small_vm], "{r}");
    }
    let container = |r: &str| of(&format!("  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }}, resources: {r} }}\n"));
    assert_eq!(container("{ cpus: 1, memory_mb: 16, disk_gb: 1 }"), Vec::<String>::new());
    let small = "machines.web.resources: too small: at least 1 cpu and 16 MB";
    assert_eq!(container("{ cpus: 0 }"), [small]);
    assert_eq!(container("{ memory_mb: 15 }"), [small]);
}

#[test]
fn an_alias_is_a_dns_name_no_other_machine_has() {
    let ok = "  web: { networks: { lan: 10 }, docker: { image: nginx }, aliases: [api-2.shop.test] }\n";
    assert_eq!(of(ok), Vec::<String>::new());
    assert_eq!(
        of("  web: { networks: { lan: 10 }, docker: { image: nginx }, aliases: [Shop.test] }\n"),
        ["machines.web.aliases[0]: `Shop.test` isn't a DNS name like api.example.com (lowercase)"]
    );
    let other = "  db: { networks: { lan: 11 }, docker: { image: postgres }, aliases: [store] }\n";
    assert_eq!(
        of(&format!(
            "{other}  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }}, aliases: [db, store] }}\n"
        )),
        [
            "machines.db.aliases[0]: `store` already names another machine",
            "machines.web.aliases[0]: `db` already names another machine",
            "machines.web.aliases[1]: `store` already names another machine"
        ]
    );
    assert_eq!(
        of("  web: { networks: { lan: 10 }, docker: { image: nginx }, aliases: [web-api, web-api] }\n"),
        Vec::<String>::new()
    );
}

#[test]
fn clones_share_no_alias() {
    let mut spec = model(&lab(
        "machines:\n  web: { networks: { lan: 10 }, docker: { image: nginx }, aliases: [shop.test] }\n",
    ))
    .unwrap();
    assert_eq!(crate::validate(&spec), Vec::new());
    if let Some(web) = spec.machines.get_mut("web") {
        web.count = Some(2);
    }
    let found: Vec<String> = crate::validate(&spec).iter().map(ToString::to_string).collect();
    assert_eq!(
        found,
        ["machines.web.aliases: clones (`count`) can't share names: give aliases to single machines"]
    );
}

#[test]
fn a_container_is_an_image_or_a_build() {
    let docker = |d: &str| of(&format!("  web: {{ networks: {{ lan: 10 }}, docker: {d} }}\n"));
    assert_eq!(docker("{ image: a, build: b }"), ["machines.web.docker: use `image` or `build`, not both"]);
    assert_eq!(
        docker("{ idle: true }"),
        ["machines.web.docker: set `image` (a published image) or `build` (a folder in the project)"]
    );
    let with_build = "machines.web.docker: `dockerfile`, `target` and `args` go with `build` (the context)";
    for extra in ["dockerfile: x", "target: prod", "args: { A: b }"] {
        assert_eq!(docker(&format!("{{ image: a, {extra} }}")), [with_build], "{extra}");
    }
    assert_eq!(docker("{ build: web, dockerfile: x, target: prod, args: { A: b } }"), Vec::<String>::new());
    assert_eq!(
        docker("{ build: web, target: '1st' }"),
        ["machines.web.docker.target: `1st` isn't a Dockerfile stage name (`FROM ... AS <name>`: letters, digits, `-`, `_` and `.`, starting with a letter)"]
    );
    assert_eq!(
        docker("{ build: web, target: 'a b' }"),
        ["machines.web.docker.target: `a b` isn't a Dockerfile stage name (`FROM ... AS <name>`: letters, digits, `-`, `_` and `.`, starting with a letter)"]
    );
    assert_eq!(docker("{ build: web, target: build-2_x.y }"), Vec::<String>::new());
}

#[test]
fn a_dynamips_router_boots_its_firmware_in_isoloom_s_container() {
    let docker = |d: &str| of(&format!("  r1: {{ networks: {{ lan: 10 }}, docker: {d} }}\n"));
    assert_eq!(docker("{ appliance: cisco-dynamips, firmware: c7200.bin }"), Vec::<String>::new());
    let own = "machines.r1.docker: a Dynamips router's container is Isoloom's: give its IOS image as `firmware`, not `image` or `build`";
    assert_eq!(docker("{ appliance: cisco-dynamips, firmware: c7200.bin, image: x }"), [own]);
    assert_eq!(docker("{ appliance: cisco-dynamips, firmware: c7200.bin, build: x }"), [own]);
    assert_eq!(
        docker("{ appliance: cisco-dynamips }"),
        ["machines.r1.docker.firmware: the IOS image (.bin) to boot, a file in the project"]
    );
    assert_eq!(
        docker("{ image: x, firmware: c7200.bin }"),
        ["machines.r1.docker.firmware: only a Dynamips router (`appliance: cisco-dynamips`) boots a firmware"]
    );
}

#[test]
fn a_generated_image_needs_no_image_or_build() {
    let mut spec = model(&lab("machines:\n  web: { networks: { lan: 10 }, docker: { image: nginx } }\n")).unwrap();
    if let Some(d) = spec.machines.get_mut("web").and_then(|m| m.docker.as_mut()) {
        d.image = None;
        d.generated = Some(crate::model::Generated {
            os: "debian-12".into(),
            base: "debian@sha256:0".into(),
            from: crate::model::BaseImage::Os,
        });
    }
    assert_eq!(crate::validate(&spec), Vec::new());
}

#[test]
fn a_vm_names_its_os_and_a_windows_one_reboots_from_playbooks() {
    let vm = |v: &str| of(&format!("  web: {{ networks: {{ lan: 10 }}, vm: {v} }}\n"));
    assert_eq!(vm("{ os: debian@12, provision: [a.sh, reboot] }"), Vec::<String>::new());
    assert_eq!(
        vm("{ os: windows-server@2022, provision: [a.ps1, reboot] }"),
        ["machines.web.vm.provision[1]: `reboot` is a Linux step: restart a Windows machine from the environment's playbooks (`ansible.windows.win_reboot`)"]
    );
    assert_eq!(
        vm("{}"),
        [
            "machines.web.vm.os: name the machine's OS with `os: <family>@<version>` (`debian@12`), here or on the machine (or share one through `common:` or a group)"
        ]
    );
}

#[test]
fn the_start_order_has_no_cycle() {
    let found = of(
        "  a: { networks: { lan: 10 }, docker: { image: x }, ports: [{ port: 80 }], depends_on: [b] }\n  b: { networks: { lan: 11 }, docker: { image: x }, ports: [{ port: 80 }], depends_on: [a] }\n",
    );
    assert_eq!(found, ["machines: depends_on forms a cycle: a -> b -> a"]);
}

#[test]
fn an_existing_host_has_an_address_a_port_and_a_user() {
    let external = |e: &str| of(&format!("  web: {{ networks: {{ lan: 10 }}, vm: {{ os: debian@12 }}, external: {e} }}\n"));
    assert_eq!(external("{ address: 192.168.1.20, port: 22, user: admin }"), Vec::<String>::new());
    let address = "machines.web.external.address: an address or hostname reachable from here, like 192.168.1.20 or dc01.lab.local";
    for bad in ["' '", "'a b'", "'root@h'", "\"a'b\"", "'a\"b'"] {
        assert_eq!(external(&format!("{{ address: {bad} }}")), [address], "{bad}");
    }
    assert_eq!(external("{ address: h, port: 0 }"), ["machines.web.external.port: 1 to 65535"]);
    for bad in ["' '", "'a@b'", "'a b'"] {
        assert_eq!(
            external(&format!("{{ address: h, user: {bad} }}")),
            ["machines.web.external.user: a user name"],
            "{bad}"
        );
    }
}
