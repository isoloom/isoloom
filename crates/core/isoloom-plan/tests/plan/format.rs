//! An environment's problems and the targets its machines' forms allow (the examples are checked in the
//! `examples` test).

use crate::testing::{ids, lab, problems};
use isoloom_spec::syntax::Code;

const BASE: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

#[test]
fn asking_for_an_impossible_target_says_why_the_lab_does() {
    let p = problems(&format!(
        "{BASE}targets: [docker]\nmachines:\n  dc: {{ networks: {{ lab: 5 }}, ports: {{ ldap: 389 }}, os: windows-server@2022, app: {{ steps: [{{ run: p.ps1 }}] }} }}\n"
    ));
    assert_eq!(
        p,
        ["targets[0]: `docker` can't run the environment: machines.dc.os: Windows has no container form"]
    );
}

#[test]
fn narrowing_targets_keeps_only_those() {
    let lab = lab(&format!(
        "{BASE}targets: [docker, vagrant]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, os: debian@12, app: {{ image: nginx, steps: [{{ run: p.sh }}] }} }}\n"
    ))
    .unwrap();
    assert_eq!(ids(lab.possible()), ["docker", "vagrant"]);
}

#[test]
fn addresses_and_networks_are_checked() {
    let p = problems(
        "version: 1\nname: t\nnetworks:\n  a: { cidr: 8.8.8.0/24 }\n  b: { cidr: 10.1.0.0/24 }\n  c: { cidr: 10.1.0.0/25 }\nmachines:\n  x: { networks: { b: 1 }, app: { image: nginx } }\n  y: { networks: { b: 7, nope: 3 }, app: { image: nginx } }\n  z: { networks: { b: 7 }, app: { image: nginx } }\n",
    );
    assert_eq!(
        p,
        [
            "networks.a.cidr: use a private block: inside 10.0.0.0/8, 172.16.0.0/12 or 192.168.0.0/16",
            "networks.c.cidr: overlaps network `b`",
            "machines.x.networks.b: 1 isn't a usable address in 10.1.0.0/24 (reserved: .0, .1 for the gateway, the two last addresses for the controller and the router, and the broadcast address)",
            "machines.y.networks.nope: no network named `nope`",
            "machines.z.networks.b: address .7 is already used by `y`"
        ]
    );
}

#[test]
fn dependencies_must_exist_and_not_loop() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, ports: {{ one: 1 }}, depends_on: [b], app: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, ports: {{ two: 2 }}, depends_on: [a], app: {{ image: x }} }}\n  c: {{ networks: {{ lab: 7 }}, depends_on: [ghost], app: {{ image: x }} }}\n"
    ));
    assert_eq!(
        p,
        [
            "machines: depends_on forms a cycle: a -> b -> a",
            "machines.c.depends_on[0]: no machine named `ghost`"
        ]
    );
}

#[test]
fn a_file_from_an_undeclared_input_is_an_unknown_reference() {
    let found = lab(&format!(
        "{BASE}inputs: [TOKEN]\nmachines:\n  a: {{ networks: {{ lab: 5 }}, files: [{{ path: /run/secret, input: SECRET }}], app: {{ image: x }} }}\n"
    ))
    .unwrap()
    .problems();
    let found: Vec<(Code, &str)> = found.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(found, [(Code::UnknownReference, "machines.a.files[0].input")]);
}

#[test]
fn a_lowered_problem_keeps_its_code() {
    let found = lab(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, depends_on: [ghost], dns: {{ options: ['no such'] }}, app: {{ image: x }} }}\n"
    ))
    .unwrap()
    .problems();
    let found: Vec<(Code, &str)> = found.iter().map(|p| (p.code, p.path.as_str())).collect();
    assert_eq!(
        found,
        [
            (Code::UnknownReference, "machines.a.depends_on[0]"),
            (Code::InvalidValue, "machines.a.dns.options[0]")
        ]
    );
}

#[test]
fn a_machine_needs_an_os_unless_it_is_the_access_machine() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }} }}\n  u: {{ access: true, networks: {{ lab: 6 }} }}\n"
    ));
    assert_eq!(p, ["machines.a.os: a machine says what it is: its OS, plus a service or an app"]);
}

#[test]
fn the_last_address_is_reserved_for_the_router() {
    let p = problems(&format!("{BASE}machines:\n  a: {{ networks: {{ lab: 254 }}, app: {{ image: x }} }}\n"));
    assert_eq!(
        p,
        [
            "machines.a.networks.lab: 254 isn't a usable address in 10.9.0.0/24 (reserved: .0, .1 for the gateway, the two last addresses for the controller and the router, and the broadcast address)"
        ]
    );
}

#[test]
fn a_gateway_is_a_machine_on_the_network_at_the_gateway_address() {
    let base = |nets: &str, fw: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n{nets}\nmachines:\n  fw: {{ networks: {fw}, app: {{ image: a }} }}\n  web: {{ networks: {{ dmz: 10 }}, app: {{ image: a }} }}\n"
        )
    };
    let ok = base("  out: { cidr: 10.1.0.0/24 }\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ out: 2, dmz: 1 }");
    assert_eq!(problems(&ok), Vec::<String>::new());

    let unknown = base("  dmz: { cidr: 10.1.1.0/24, gateway: nope }", "{ dmz: 2 }");
    assert_eq!(problems(&unknown), ["networks.dmz.gateway: no machine named `nope`"], "an unknown gateway");
    let detached = base("  out: { cidr: 10.1.0.0/24 }\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ out: 2 }");
    assert_eq!(
        problems(&detached),
        ["networks.dmz.gateway: attach `fw` to `dmz` at the gateway address: `networks: { dmz: 1 }`"],
        "a gateway off its network"
    );
    let wrong_octet = base("  dmz: { cidr: 10.1.1.0/24, gateway: fw }", "{ dmz: 5 }");
    assert_eq!(
        problems(&wrong_octet),
        ["machines.fw.networks.dmz: `fw` is this network's gateway, so it takes the gateway address: use 1"],
        "a gateway at another address"
    );
    // Only the gateway may take .1.
    let squatter = "version: 1\nname: t\nnetworks:\n  a: { cidr: 10.1.0.0/24 }\nmachines:\n  m: { networks: { a: 1 }, app: { image: a } }\n";
    assert_eq!(
        problems(squatter),
        [
            "machines.m.networks.a: 1 isn't a usable address in 10.1.0.0/24 (reserved: .0, .1 for the gateway, the two last addresses for the controller and the router, and the broadcast address)"
        ],
        "a machine at .1 without a gateway"
    );
}

#[test]
fn a_gateway_waiting_for_a_machine_behind_it_is_a_cycle() {
    let looped = "version: 1\nname: t\nnetworks:\n  dmz: { cidr: 10.1.1.0/24, gateway: fw }\nmachines:\n  fw: { networks: { dmz: 1 }, depends_on: [web], app: { image: a } }\n  web: { networks: { dmz: 10 }, ports: { web: 80 }, app: { image: a } }\n";
    assert_eq!(
        problems(looped),
        ["machines: depends_on forms a cycle: fw -> web -> fw (machines on a gateway's network also start after it)"]
    );
}

#[test]
fn volumes_are_absolute_paths_named_in_kebab_case() {
    let p = problems(&format!(
        "{BASE}machines:\n  a: {{ networks: {{ lab: 5 }}, volumes: {{ data: /data, Bad: /x, rel: data/x, twice: /data/, spaced: \"/var/my data\", coloned: \"/var/a:b\" }}, app: {{ image: x }} }}\n"
    ));
    // Spaces and ':' corrupt the Docker short mount syntax and the provisioners' `mkdir -p`.
    assert_eq!(
        p,
        [
            "machines.a.volumes.Bad: volume names are kebab-case: lowercase letters, digits and dashes",
            "machines.a.volumes.rel: `data/x` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data",
            "machines.a.volumes.twice: `/data/` is already a volume of this machine",
            "machines.a.volumes.spaced: `/var/my data` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data",
            "machines.a.volumes.coloned: `/var/a:b` isn't a safe absolute path inside the machine (no spaces or ':'), like /var/lib/postgresql/data"
        ]
    );
}

#[test]
fn names_leave_room_for_the_kubernetes_affixes() {
    // The name becomes `isoloom-<name>` (namespace): 56 characters overflows the 63-char label.
    let long = "a".repeat(56);
    let p = problems(&format!(
        "version: 1\nname: {long}\nnetworks:\n  lab: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  a: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\n"
    ));
    assert_eq!(
        p,
        ["name: at most 55 characters: it becomes the Kubernetes namespace `isoloom-<name>`"],
        "a long lab name"
    );

    // A 56-char machine name overflows `isoloom-<name>`; a machine+volume pair overflows the claim.
    let m56 = "m".repeat(56);
    let v63 = "v".repeat(63);
    let overflow = problems(&format!(
        "{BASE}machines:\n  {m56}: {{ networks: {{ lab: 5 }}, app: {{ image: x }} }}\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v63}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert_eq!(
        overflow,
        [
            format!("machines.{m56}: at most 55 characters: its published ports are the Kubernetes Service `isoloom-<name>`"),
            format!("machines.b.volumes.{v63}: machine name and volume name together are at most 62 characters: the Kubernetes claim is `<machine>-<volume>`")
        ],
        "a long machine name, a long volume name"
    );
    // A volume name that fits the claim but not its seed init container (`seed-<volume>`).
    let v59 = "v".repeat(59);
    let seeded = problems(&format!(
        "{BASE}machines:\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v59}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert_eq!(
        seeded,
        [format!(
            "machines.b.volumes.{v59}: volume names are at most 58 characters: the Kubernetes init container seeding it is `seed-<volume>`"
        )],
        "a volume name too long for its seed"
    );
    let v58 = "v".repeat(58);
    let fits = problems(&format!(
        "{BASE}machines:\n  b: {{ networks: {{ lab: 6 }}, volumes: {{ {v58}: /data }}, app: {{ image: x }} }}\n"
    ));
    assert_eq!(fits, Vec::<String>::new(), "a volume name that fits");
}

/// A container can be small (a Go service capped at 64 MB); a VM needs room to boot.
#[test]
fn the_size_floor_is_a_vms() {
    let spec = |app: &str| {
        format!(
            "version: 1\nname: t\nnetworks:\n  lan: {{ cidr: 10.9.0.0/24 }}\nmachines:\n  svc:\n    networks: {{ lan: 10 }}\n    resources: {{ cpus: 1, memory_mb: 64 }}\n{app}"
        )
    };
    assert_eq!(problems(&spec("    app: { image: nginx }\n")), Vec::<String>::new());
    let both = problems(&spec("    os: debian@12\n    app: { image: nginx, steps: [{ run: x.sh }] }\n"));
    assert_eq!(both, ["machines.svc.resources: too small for a VM: at least 1 cpu, 256 MB and 5 GB"], "a VM");
    let tiny = problems(&spec("    app: { image: nginx }\n").replace("memory_mb: 64", "memory_mb: 8"));
    assert_eq!(tiny, ["machines.svc.resources: too small: at least 1 cpu and 16 MB"], "a container");
}
