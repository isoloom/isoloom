//! An environment's problems, each spec with one mistake: every problem names the field the author wrote
//! and says what is wrong, so an author can fix it without reading the generators.

use crate::testing::{assert_problem, problems};
use isoloom_plan::Lab;
use isoloom_spec::syntax::{self, Problem};

/// A spec with one network, `lab` (10.9.0.0/24), and nothing else.
const NET: &str = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\n";

/// A valid machine on `lab`.
const WEB: &str = "{ networks: { lab: 5 }, app: { image: nginx } }";

/// `NET` with these lines under `machines:` (each `  name: { … }`).
fn with_machines(lines: &[&str]) -> String {
    let body: Vec<String> = lines.iter().map(|l| format!("  {l}\n")).collect();
    format!("{NET}machines:\n{}", body.concat())
}

/// `NET` with one machine, `m`.
fn with_machine(body: &str) -> String {
    with_machines(&[&format!("m: {body}")])
}

/// A valid machine with `web` and these lines appended after `machines:` (top-level keys).
fn with_web_and(extra: &str) -> String {
    format!("{}{extra}", with_machines(&[&format!("web: {WEB}")]))
}

/// `NET` (with `web`) with a declared check.
fn with_check(check: &str) -> String {
    with_web_and(&format!("checks:\n  - {check}\n"))
}

/// Each `(spec, path, fragment)`: some problem of the spec is at `path` and says `fragment`.
#[track_caller]
fn assert_problems(cases: &[(String, &str, &str)]) {
    for (yaml, at, fragment) in cases {
        assert_problem(yaml, at, fragment);
    }
}

#[test]
fn the_specs_own_mistakes_are_named_at_their_field() {
    assert_problems(&[
        (
            with_machine(WEB).replace("version: 1", "version: 2"),
            "version",
            "version 2 isn't one this isoloom reads",
        ),
        (with_machine(WEB).replace("name: t", "name: Bad_Name"), "name", "kebab-case"),
        (
            "version: 1\nname: t\nnetworks: {}\nmachines:\n  m: { networks: {}, app: { image: nginx } }\n".to_owned(),
            "networks",
            "at least one network",
        ),
        (NET.to_owned(), "machines", "at least one machine"),
        (with_web_and("inputs: [1TOKEN]\n"), "inputs[0]", "named like an environment variable"),
        (
            "version: 1\nname: t\ncloud: { provider: aws, terraform: tf }\n".to_owned(),
            "cloud",
            "cloud-services is not supported in Isoloom 3.0",
        ),
    ]);
}

/// `NET` with machine `m` and the docker targets' block for `lab`.
fn with_docker_cidr(cidr: &str) -> String {
    format!("{}overrides:\n  docker: {{ networks: {{ lab: {{ cidr: {cidr} }} }} }}\n", with_machine(WEB))
}

#[test]
fn a_networks_mistakes_are_named_at_their_field() {
    assert_problems(&[
        (
            "version: 1\nname: t\nnetworks:\n  Lab_1: { cidr: 10.9.0.0/24 }\nmachines:\n  m: { networks: { Lab_1: 5 }, app: { image: nginx } }\n".to_owned(),
            "networks.Lab_1",
            "kebab-case",
        ),
        (with_machine(WEB).replace("10.9.0.0/24", "10.9.0.5/24"), "networks.lab.cidr", "isn't a network address"),
        (with_machine(WEB).replace("10.9.0.0/24", "10.9.0.0/16"), "networks.lab.cidr", "/24 to /29"),
        (with_docker_cidr("10.8.0.0/25"), "overrides.docker.networks.lab.cidr", "the same size as `cidr`"),
        (with_docker_cidr("nope"), "overrides.docker.networks.lab.cidr", "isn't a network address"),
        (
            "version: 1\nname: t\nnetworks:\n  a: { cidr: 10.9.0.0/24 }\n  b: { cidr: 10.8.0.0/24 }\nmachines:\n  m: { networks: { a: 5, b: 5 }, app: { image: nginx } }\noverrides:\n  docker: { networks: { a: { cidr: 10.8.0.0/24 } } }\n".to_owned(),
            "overrides.docker.networks.b.cidr",
            "on Docker, overlaps network `a`",
        ),
        (with_web_and("reach: [{ from: lab, to: lab }]\n"), "reach[0]", "same network always reach each other"),
        (with_web_and("reach: [{ from: lab, to: lab, ports: [0] }]\n"), "reach[0].ports", "port 0 isn't a port"),
    ]);
}

/// `NET` with machine `m`, `lab` impaired by `tc`.
fn with_tc(tc: &str) -> String {
    with_machine(WEB).replace("cidr: 10.9.0.0/24 }", &format!("cidr: 10.9.0.0/24, tc: {tc} }}"))
}

#[test]
fn a_links_impairment_mistakes_are_named_at_their_field() {
    let at = |field: &str| format!("networks.lab.tc.{field}");
    let (delay, jitter, loss, rate) = (at("delay"), at("jitter"), at("loss"), at("rate"));
    assert_problems(&[
        (with_tc("{ delay: fast }"), &delay, "isn't a time like 50ms"),
        (with_tc("{ delay: 50ms, jitter: x }"), &jitter, "isn't a time like 5ms"),
        (with_tc("{ jitter: 5ms }"), &jitter, "varies a `delay`"),
        (with_tc("{ loss: 150 }"), &loss, "0 to 100"),
        (with_tc("{ loss: -1 }"), &loss, "0 to 100"),
        (with_tc("{ delay: -5ms }"), &delay, "can't be negative"),
        (with_tc("{ delay: 50ms, jitter: -1ms }"), &jitter, "can't be negative"),
        (with_tc("{ rate: -10mbit }"), &rate, "can't be negative"),
        (with_tc("{ rate: fast }"), &rate, "isn't a rate like 10mbit"),
        (
            with_machines(&["gw: { networks: { lab: 1 }, app: { image: router } }"])
                .replace("cidr: 10.9.0.0/24 }", "cidr: 10.9.0.0/24, gateway: gw, tc: { delay: 50ms } }"),
            "networks.lab.tc",
            "routed by its gateway machine",
        ),
    ]);
}

#[test]
fn a_machines_mistakes_are_named_at_their_field() {
    assert_problems(&[
        (with_machines(&[&format!("Web_1: {WEB}")]), "machines.Web_1", "DNS names"),
        (
            with_machine("{ networks: {}, app: { image: nginx } }"),
            "machines.m.networks",
            "at least one network",
        ),
        (
            with_machines(&["a: { networks: { lab: 5 }, access: true }", "b: { networks: { lab: 6 }, access: true }"]),
            "machines",
            "only one machine can be the access machine (found 2)",
        ),
        (
            with_machines(&[
                "a: { networks: { lab: 5 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }",
                "b: { networks: { lab: 6 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }",
            ]),
            "machines.b.ports.web.publish",
            "already published by `a`",
        ),
        (
            with_machine("{ networks: { lab: 5 }, ports: { web: { port: 80, publish: { fixed: true } } }, app: { image: nginx } }"),
            "machines.m.ports.web.publish.host",
            "`host` is required here",
        ),
        (
            with_machine("{ networks: { lab: 5 }, depends_on: [m], app: { image: nginx } }"),
            "machines.m.depends_on[0]",
            "can't depend on itself",
        ),
        (
            with_machines(&[
                "a: { networks: { lab: 5 }, depends_on: [b], app: { image: nginx } }",
                "b: { networks: { lab: 6 }, app: { image: nginx } }",
            ]),
            "machines.a.depends_on[0]",
            "nothing to wait for",
        ),
        (
            with_machine("{ networks: { lab: 5 }, resources: { memory_mb: 8 }, app: { image: nginx } }"),
            "machines.m.resources",
            "at least 1 cpu and 16 MB",
        ),
        // A VM-only machine beside a container-only access machine: no container target takes
        // the VM, and no VM target the container.
        (
            with_machines(&[
                "a: { networks: { lab: 5 }, os: windows-server@2022, app: { steps: [{ run: a.ps1 }] } }",
                "b: { networks: { lab: 6 }, access: true, app: { image: nginx } }",
            ]),
            "machines",
            "no target is possible: machines.a.os: Windows has no container form; machines.b.app.image: ",
        ),
    ]);
}

#[test]
fn a_machines_aliases_mistakes_are_named_at_their_field() {
    assert_problems(&[
        (
            with_machine("{ networks: { lab: 5 }, aliases: [Bad], app: { image: nginx } }"),
            "machines.m.aliases[0]",
            "isn't a DNS name",
        ),
        (
            with_machines(&[
                "a: { networks: { lab: 5 }, aliases: [b], app: { image: nginx } }",
                "b: { networks: { lab: 6 }, app: { image: nginx } }",
            ]),
            "machines.a.aliases[0]",
            "already names another machine",
        ),
        // Clones are expanded before validation, so each clone claims the alias the others
        // have: the problem is at the counted machine.
        (
            with_machine("{ networks: { lab: 5 }, count: 2, aliases: [api.lab], app: { image: nginx } }"),
            "machines.m-02.aliases[0]",
            "`api.lab` already names another machine",
        ),
    ]);
}

#[test]
fn an_image_beside_a_build_and_a_bare_os_are_valid_machines() {
    for (case, body) in [
        // `image` with `build` is legal: the build is tagged as that published name (built in
        // development, pulled in production), the way Docker Compose's `image` + `build` combine.
        ("an image beside a build", "{ networks: { lab: 5 }, app: { image: ghcr.io/x/y:1, build: web } }"),
        // An OS alone is a machine (a fresh VM, or the OS's image kept running idle): no steps needed.
        ("a bare os", "{ networks: { lab: 5 }, os: debian@12 }"),
    ] {
        assert_eq!(problems(&with_machine(body)), Vec::<String>::new(), "{case}");
    }
}

/// The recipe of an appliance, Cisco IOL.
const IOL: &str = "appliance: cisco-iol, image: 'vrnetlab/cisco_iol:17.12.01'";

#[test]
fn a_recipes_mistakes_are_named_at_their_field() {
    assert_problems(&[
        (with_machine("{ networks: { lab: 5 }, app: {} }"), "machines.m.app", "needs a recipe"),
        (
            with_machine("{ networks: { lab: 5 }, app: { appliance: cisco-dynamips, image: x, firmware: ios.bin } }"),
            "machines.m.app",
            "so no `image`",
        ),
        (
            with_machine("{ networks: { lab: 5 }, app: { appliance: cisco-dynamips } }"),
            "machines.m.app",
            "needs the `firmware` it boots",
        ),
        (
            with_machine("{ networks: { lab: 5 }, app: { image: nginx, firmware: ios.bin } }"),
            "machines.m.app",
            "it goes with `appliance: cisco-dynamips`",
        ),
        (
            with_machine(&format!("{{ networks: {{ lab: 5 }}, access: true, app: {{ {IOL} }} }}")),
            "machines.m.access",
            "can't be the access machine",
        ),
        (
            with_machine(&format!("{{ networks: {{ lab: 5 }}, app: {{ {IOL}, config: ../r1.cfg }} }}")),
            "machines.m.app.config",
            "a path in the project",
        ),
        (
            with_machine("{ networks: { lab: 5 }, app: { steps: [{ run: p.sh }] } }"),
            "machines.m.os",
            "steps install onto an OS",
        ),
        (
            with_machine("{ networks: { lab: 5 }, os: plan9, app: { steps: [{ run: p.sh }] } }"),
            "machines.m.os",
            "unknown OS `plan9`",
        ),
    ]);
}

/// A VM on `lab`.
const VM: &str = "{ networks: { lab: 5 }, os: debian@12 }";

/// `NET` with the VM `m` and its existing host's `ssh` block of `fields`.
fn external(fields: &str) -> String {
    format!(
        "{}overrides:\n  external: {{ machines: {{ m: {{ ssh: {{ {fields} }} }} }} }}\n",
        with_machine(VM)
    )
}

#[test]
fn provisioning_and_existing_hosts_mistakes_are_named_at_their_field() {
    let ssh = "overrides.external.machines.m.ssh";
    assert_problems(&[
        (
            format!("{}provision: [{{ ansible: site.txt }}]\n", with_machine(VM)),
            "provision[0].ansible",
            "a .yml or .yaml file",
        ),
        (
            format!("{}provision: [{{ ansible: site.yml, groups: {{ all: [m] }} }}]\n", with_machine(VM)),
            "provision[0].groups.all",
            "Isoloom fills this group itself",
        ),
        (external("address: 'a b'"), ssh, "reachable from here"),
        (external("address: 10.0.0.5, port: 0"), ssh, "1 to 65535"),
        (external("address: 10.0.0.5, user: 'a@b'"), ssh, "a user name"),
    ]);
}

#[test]
fn a_tools_mistakes_are_named_at_their_field() {
    let tools = |t: &str| with_web_and(&format!("tools:\n{t}"));
    assert_problems(&[
        (tools("  Bad_T: { image: alpine }\n"), "tools.Bad_T", "kebab-case"),
        (tools("  web: { image: alpine }\n"), "tools.web", "already a machine's or a group's name"),
        (tools("  toolbox: {}\n"), "tools.toolbox.image", "use a recipe"),
        (tools("  shell: { image: alpine }\n"), "tools.shell.image", "is a recipe with its own image"),
        (tools("  toolbox: { image: alpine, publish: 9000 }\n"), "tools.toolbox.publish", "which `port`"),
        (
            format!(
                "{}tools:\n  toolbox: {{ image: alpine, port: 80, publish: 8080 }}\n",
                with_machine("{ networks: { lab: 5 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }")
            ),
            "tools.toolbox.publish",
            "already published by a machine",
        ),
        (
            tools("  a: { image: x }\n  b: { image: x }\n  c: { image: x }\n  d: { image: x }\n"),
            "tools",
            "at most 3 tools",
        ),
    ]);
}

#[test]
fn a_messages_mistakes_are_named_at_the_message() {
    assert_problems(&[
        (
            with_web_and("message: 'at {{ machines..web }}'\n"),
            "message",
            "isn't a placeholder Isoloom knows",
        ),
        (
            with_web_and("message: 'at {{ machines.db.addresses.lab }}'\n"),
            "message",
            "no machine named `db`",
        ),
    ]);
}

#[test]
fn a_checks_mistakes_are_named_at_their_field() {
    let from = |machine: &str, check: &str| {
        format!(
            "{}checks:\n  - {check}\n",
            with_machines(&["web: { networks: { lab: 5 }, os: debian@12, app: { steps: [{ run: p.sh }] } }", machine])
        )
    };
    assert_problems(&[
        (with_web_and("checks: ['']\n"), "checks[0]", "give the script's path"),
        (with_check("{ name: ' ', tcp: 'web:80' }"), "checks[0].name", "give the check a name"),
        (
            from(&format!("r1: {{ networks: {{ lab: 6 }}, app: {{ {IOL} }} }}"), "{ from: r1, tcp: 'web:80' }"),
            "checks[0].from",
            "is a network appliance",
        ),
        (
            from(
                "win: { networks: { lab: 6 }, os: windows-server@2022, app: { steps: [{ run: p.ps1 }] } }",
                "{ from: win, tcp: 'web:80' }",
            ),
            "checks[0].from",
            "runs Windows",
        ),
        (with_check("{ tcp: 'web:80', method: POST }"), "checks[0]", "go with `http`"),
        (
            with_check("{ http: 'http://web/', contains: hi, expect: blocked }"),
            "checks[0]",
            "sends nothing to look at",
        ),
        (with_check("{ http: 'http://web/', method: 'G T' }"), "checks[0].method", "isn't an HTTP method"),
        (with_check("{ from: web, exec: ' ' }"), "checks[0].exec", "give a command to run"),
        (
            with_check("{ from: web, exec: id, expect: 200 }"),
            "checks[0].expect",
            "the text the output must contain",
        ),
        (with_check("{ script: checks/a.sh, expect: open }"), "checks[0].expect", "exiting 0"),
    ]);
}

#[test]
fn a_build_outside_the_project_folder_is_named_at_its_field() {
    let dir = tempfile::tempdir().unwrap();
    let yaml = with_machine("{ networks: { lab: 5 }, app: { build: ../web } }");
    let found = match syntax::parse(&yaml) {
        Ok(spec) => Lab::new(spec).problems_with_files(dir.path()),
        Err(problems) => problems,
    };
    assert_eq!(
        found.iter().map(Problem::line).collect::<Vec<_>>(),
        ["machines.m.app.build: `../web` must be a path inside the project folder: it goes up a folder (`..`)"]
    );
}

#[test]
fn a_catalogue_service_names_an_entry_a_release_an_os_and_settings_it_has() {
    let service = |s: &str| with_machine(&format!("{{ networks: {{ lab: 5 }}, {s} }}"));
    assert_problems(&[
        (
            service("os: debian@12, service: nginz"),
            "machines.m.service",
            "the catalogue has no entry `nginz`",
        ),
        (service("os: debian@12, service: nginx@0.9"), "machines.m.service", "nginx has no release `0.9`"),
        (service("os: kali, service: nginx"), "machines.m.os", "nginx 1.30.6 doesn't run on kali@rolling"),
        (
            service("os: debian@12, service: nginx, with: { prot: 81 }"),
            "machines.m.with.prot",
            "nginx has no setting `prot`",
        ),
        (
            service("os: debian@12, service: nginx, with: { port: http }"),
            "machines.m.with.port",
            "`port` is a port number",
        ),
        (
            service("os: debian@12, service: nginx, with: { php_fpm: 'nope:9000' }"),
            "machines.m.with.php_fpm",
            "no machine named `nope`",
        ),
    ]);
    assert_eq!(
        problems(&service("os: debian@12, service: nginx@1.31, with: { port: 8080 }")),
        Vec::<String>::new()
    );
}
