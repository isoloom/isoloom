//! A lab's problems, each spec with one mistake: every problem names the field the author wrote
//! and says what is wrong, so an author can fix it without reading the generators.

use std::path::{Path, PathBuf};

use isoloom_plan::Lab;
use isoloom_spec::syntax::{self, Problem};
use testing::{assert_problem, problems};

pub mod testing;

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

/// A cloud-services spec with these fields under `cloud:` (besides the provider).
fn cloud(fields: &str) -> String {
    format!("version: 1\nname: t\ncloud:\n  provider: aws\n{fields}")
}

/// `NET` (with `web`) with a declared check.
fn with_check(check: &str) -> String {
    with_web_and(&format!("checks:\n  - {check}\n"))
}

// The spec itself.

#[test]
fn version_other_than_1() {
    assert_problem(
        &with_machine(WEB).replace("version: 1", "version: 2"),
        "version",
        "version 2 isn't one this isoloom reads",
    );
}

#[test]
fn name_not_kebab_case() {
    assert_problem(&with_machine(WEB).replace("name: t", "name: Bad_Name"), "name", "kebab-case");
}

#[test]
fn no_network() {
    assert_problem(
        "version: 1\nname: t\nnetworks: {}\nmachines:\n  m: { networks: {}, app: { image: nginx } }\n",
        "networks",
        "at least one network",
    );
}

#[test]
fn no_machine() {
    assert_problem(NET, "machines", "at least one machine");
}

#[test]
fn input_not_an_env_var_name() {
    assert_problem(&with_web_and("inputs: [1TOKEN]\n"), "inputs[0]", "named like an environment variable");
}

// Cloud services.

#[test]
fn cloud_with_machines() {
    let yaml = format!("{}machines:\n  m: {WEB}\n", cloud("  terraform: tf\n"));
    assert_problem(&yaml, "cloud", "not both");
}

#[test]
fn cloud_without_module() {
    assert_problem(&cloud("  terraform: ' '\n"), "cloud.terraform", "Terraform module's folder");
}

#[test]
fn cloud_var_not_a_terraform_name() {
    assert_problem(
        &cloud("  terraform: tf\n  vars: { '1x': a }\n"),
        "cloud.vars.1x",
        "isn't a Terraform variable name",
    );
}

#[test]
fn cloud_output_not_a_name() {
    assert_problem(
        &cloud("  terraform: tf\n  outputs: { site: 'a b' }\n"),
        "cloud.outputs.site",
        "the module output it reads",
    );
}

#[test]
fn cloud_var_from_undeclared_input() {
    let yaml = cloud("  terraform: tf\n  vars: { cidr: '{{ inputs.CIDR }}' }\n");
    assert_problem(&yaml, "cloud.vars.cidr", "`CIDR` isn't declared");
}

#[test]
fn cloud_hourly_cost_out_of_range() {
    assert_problem(&cloud("  terraform: tf\n  hourly_usd: 5000\n"), "cloud.hourly_usd", "0 to 1000");
}

// Networks.

#[test]
fn network_name_not_kebab_case() {
    let yaml = "version: 1\nname: t\nnetworks:\n  Lab_1: { cidr: 10.9.0.0/24 }\nmachines:\n  m: { networks: { Lab_1: 5 }, app: { image: nginx } }\n";
    assert_problem(yaml, "networks.Lab_1", "kebab-case");
}

#[test]
fn cidr_not_a_network_address() {
    assert_problem(
        &with_machine(WEB).replace("10.9.0.0/24", "10.9.0.5/24"),
        "networks.lab.cidr",
        "isn't a network address",
    );
}

#[test]
fn cidr_too_large() {
    assert_problem(&with_machine(WEB).replace("10.9.0.0/24", "10.9.0.0/16"), "networks.lab.cidr", "/24 to /29");
}

/// `NET` with machine `m` and the docker targets' block for `lab`.
fn with_docker_cidr(cidr: &str) -> String {
    format!("{}overrides:\n  docker: {{ networks: {{ lab: {{ cidr: {cidr} }} }} }}\n", with_machine(WEB))
}

#[test]
fn docker_cidr_of_another_size() {
    assert_problem(
        &with_docker_cidr("10.8.0.0/25"),
        "overrides.docker.networks.lab.cidr",
        "the same size as `cidr`",
    );
}

#[test]
fn docker_cidr_not_a_network_address() {
    assert_problem(&with_docker_cidr("nope"), "overrides.docker.networks.lab.cidr", "isn't a network address");
}

#[test]
fn networks_overlapping_once_moved_on_docker() {
    let yaml = "version: 1\nname: t\nnetworks:\n  a: { cidr: 10.9.0.0/24 }\n  b: { cidr: 10.8.0.0/24 }\nmachines:\n  m: { networks: { a: 5, b: 5 }, app: { image: nginx } }\noverrides:\n  docker: { networks: { a: { cidr: 10.8.0.0/24 } } }\n";
    assert_problem(yaml, "overrides.docker.networks.b.cidr", "on Docker, overlaps network `a`");
}

#[test]
fn reach_within_one_network() {
    assert_problem(
        &with_web_and("reach: [{ from: lab, to: lab }]\n"),
        "reach[0]",
        "same network always reach each other",
    );
}

#[test]
fn reach_port_0() {
    assert_problem(
        &with_web_and("reach: [{ from: lab, to: lab, ports: [0] }]\n"),
        "reach[0].ports",
        "port 0 isn't a port",
    );
}

// Link impairment.

fn with_tc(tc: &str) -> String {
    with_machine(WEB).replace("cidr: 10.9.0.0/24 }", &format!("cidr: 10.9.0.0/24, tc: {tc} }}"))
}

#[test]
fn tc_delay_not_a_time() {
    assert_problem(&with_tc("{ delay: fast }"), "networks.lab.tc.delay", "isn't a time like 50ms");
}

#[test]
fn tc_jitter_not_a_time() {
    assert_problem(&with_tc("{ delay: 50ms, jitter: x }"), "networks.lab.tc.jitter", "isn't a time like 5ms");
}

#[test]
fn tc_jitter_without_delay() {
    assert_problem(&with_tc("{ jitter: 5ms }"), "networks.lab.tc.jitter", "varies a `delay`");
}

#[test]
fn tc_loss_over_100() {
    assert_problem(&with_tc("{ loss: 150 }"), "networks.lab.tc.loss", "0 to 100");
}

#[test]
fn tc_values_are_never_negative() {
    assert_problem(&with_tc("{ delay: -5ms }"), "networks.lab.tc.delay", "can't be negative");
    assert_problem(&with_tc("{ delay: 50ms, jitter: -1ms }"), "networks.lab.tc.jitter", "can't be negative");
    assert_problem(&with_tc("{ rate: -10mbit }"), "networks.lab.tc.rate", "can't be negative");
    assert_problem(&with_tc("{ loss: -1 }"), "networks.lab.tc.loss", "0 to 100");
}

#[test]
fn tc_rate_not_a_rate() {
    assert_problem(&with_tc("{ rate: fast }"), "networks.lab.tc.rate", "isn't a rate like 10mbit");
}

#[test]
fn tc_on_a_gateway_network() {
    let yaml = with_machines(&["gw: { networks: { lab: 1 }, app: { image: router } }"])
        .replace("cidr: 10.9.0.0/24 }", "cidr: 10.9.0.0/24, gateway: gw, tc: { delay: 50ms } }");
    assert_problem(&yaml, "networks.lab.tc", "routed by its gateway machine");
}

// Machines.

#[test]
fn machine_name_not_a_dns_name() {
    assert_problem(&with_machines(&[&format!("Web_1: {WEB}")]), "machines.Web_1", "DNS names");
}

#[test]
fn machine_on_no_network() {
    assert_problem(
        &with_machine("{ networks: {}, app: { image: nginx } }"),
        "machines.m.networks",
        "at least one network",
    );
}

#[test]
fn two_access_machines() {
    let yaml = with_machines(&["a: { networks: { lab: 5 }, access: true }", "b: { networks: { lab: 6 }, access: true }"]);
    assert_problem(&yaml, "machines", "only one machine can be the access machine (found 2)");
}

#[test]
fn port_published_twice() {
    let yaml = with_machines(&[
        "a: { networks: { lab: 5 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }",
        "b: { networks: { lab: 6 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }",
    ]);
    assert_problem(&yaml, "machines.b.ports.web.publish", "already published by `a`");
}

#[test]
fn fixed_without_a_host_port() {
    let yaml = with_machine("{ networks: { lab: 5 }, ports: { web: { port: 80, publish: { fixed: true } } }, app: { image: nginx } }");
    assert_problem(&yaml, "machines.m.ports.web.publish.host", "`host` is required here");
}

#[test]
fn depends_on_itself() {
    let yaml = with_machine("{ networks: { lab: 5 }, depends_on: [m], app: { image: nginx } }");
    assert_problem(&yaml, "machines.m.depends_on[0]", "can't depend on itself");
}

#[test]
fn depends_on_a_machine_without_services() {
    let yaml = with_machines(&[
        "a: { networks: { lab: 5 }, depends_on: [b], app: { image: nginx } }",
        "b: { networks: { lab: 6 }, app: { image: nginx } }",
    ]);
    assert_problem(&yaml, "machines.a.depends_on[0]", "nothing to wait for");
}

#[test]
fn container_too_small() {
    let yaml = with_machine("{ networks: { lab: 5 }, resources: { memory_mb: 8 }, app: { image: nginx } }");
    assert_problem(&yaml, "machines.m.resources", "at least 1 cpu and 16 MB");
}

#[test]
fn alias_not_a_dns_name() {
    let yaml = with_machine("{ networks: { lab: 5 }, aliases: [Bad], app: { image: nginx } }");
    assert_problem(&yaml, "machines.m.aliases[0]", "isn't a DNS name");
}

#[test]
fn alias_naming_another_machine() {
    let yaml = with_machines(&[
        "a: { networks: { lab: 5 }, aliases: [b], app: { image: nginx } }",
        "b: { networks: { lab: 6 }, app: { image: nginx } }",
    ]);
    assert_problem(&yaml, "machines.a.aliases[0]", "already names another machine");
}

#[test]
fn aliases_on_clones() {
    // Clones are expanded before validation, so each clone claims the alias the others have.
    let yaml = with_machine("{ networks: { lab: 5 }, count: 2, aliases: [api.lab], app: { image: nginx } }");
    assert_problem(&yaml, "machines.m-02.aliases[0]", "`api.lab` already names another machine");
}

#[test]
fn a_machine_with_no_form_leaves_no_target() {
    let yaml = with_machines(&[
        "a: { networks: { lab: 5 }, app: { image: nginx } }",
        "b: { networks: { lab: 6 }, os: debian@12, hardware: { tpm: required, devices: [/dev/fuse] } }",
    ]);
    assert_problem(&yaml, "machines", "no target is possible");
}

// The container recipe.

#[test]
fn image_and_build() {
    assert_problem(
        &with_machine("{ networks: { lab: 5 }, app: { image: nginx, build: web } }"),
        "machines.m.app",
        "both given",
    );
}

#[test]
fn an_app_without_a_recipe() {
    assert_problem(&with_machine("{ networks: { lab: 5 }, app: {} }"), "machines.m.app", "needs a recipe");
}

#[test]
fn dynamips_with_an_image() {
    let yaml = with_machine("{ networks: { lab: 5 }, app: { appliance: cisco-dynamips, image: x, firmware: ios.bin } }");
    assert_problem(&yaml, "machines.m.app", "so no `image`");
}

#[test]
fn dynamips_without_firmware() {
    let yaml = with_machine("{ networks: { lab: 5 }, app: { appliance: cisco-dynamips } }");
    assert_problem(&yaml, "machines.m.app", "needs the `firmware` it boots");
}

#[test]
fn firmware_without_dynamips() {
    let yaml = with_machine("{ networks: { lab: 5 }, app: { image: nginx, firmware: ios.bin } }");
    assert_problem(&yaml, "machines.m.app", "it goes with `appliance: cisco-dynamips`");
}

// Network appliances.

const IOL: &str = "appliance: cisco-iol, image: 'vrnetlab/cisco_iol:17.12.01'";

#[test]
fn appliance_as_access_machine() {
    let yaml = with_machine(&format!("{{ networks: {{ lab: 5 }}, access: true, app: {{ {IOL} }} }}"));
    assert_problem(&yaml, "machines.m.access", "can't be the access machine");
}

#[test]
fn appliance_config_outside_the_project() {
    let yaml = with_machine(&format!("{{ networks: {{ lab: 5 }}, app: {{ {IOL}, config: ../r1.cfg }} }}"));
    assert_problem(&yaml, "machines.m.app.config", "a path in the project");
}

// VMs.

#[test]
fn steps_without_os() {
    assert_problem(
        &with_machine("{ networks: { lab: 5 }, app: { steps: [{ run: p.sh }] } }"),
        "machines.m.os",
        "steps install onto an OS",
    );
}

#[test]
fn unknown_os() {
    let yaml = with_machine("{ networks: { lab: 5 }, os: plan9, app: { steps: [{ run: p.sh }] } }");
    assert_problem(&yaml, "machines.m.os", "unknown OS `plan9`");
}

#[test]
fn a_bare_os_is_a_machine_of_its_own() {
    // An OS alone is a machine (a fresh VM, or the OS's image kept running idle): no steps needed.
    assert_eq!(problems(&with_machine("{ networks: { lab: 5 }, os: debian@12 }")), Vec::<String>::new());
}

// Environment provisioning.

const VM: &str = "{ networks: { lab: 5 }, os: debian@12 }";

#[test]
fn provision_not_a_playbook() {
    let yaml = format!("{}provision: [{{ ansible: site.txt }}]\n", with_machine(VM));
    assert_problem(&yaml, "provision[0].ansible", "a .yml or .yaml file");
}

#[test]
fn provision_group_isoloom_fills() {
    let yaml = format!("{}provision: [{{ ansible: site.yml, groups: {{ all: [m] }} }}]\n", with_machine(VM));
    assert_problem(&yaml, "provision[0].groups.all", "Isoloom fills this group itself");
}

// Tools.

#[test]
fn tool_name_not_kebab_case() {
    assert_problem(&with_web_and("tools:\n  Bad_T: { image: alpine }\n"), "tools.Bad_T", "kebab-case");
}

#[test]
fn tool_named_like_a_machine() {
    assert_problem(
        &with_web_and("tools:\n  web: { image: alpine }\n"),
        "tools.web",
        "already a machine's or a group's name",
    );
}

#[test]
fn tool_without_image() {
    assert_problem(&with_web_and("tools:\n  kali: {}\n"), "tools.kali.image", "use a recipe");
}

#[test]
fn recipe_with_an_image() {
    assert_problem(
        &with_web_and("tools:\n  shell: { image: alpine }\n"),
        "tools.shell.image",
        "is a recipe with its own image",
    );
}

#[test]
fn tool_publish_without_port() {
    assert_problem(
        &with_web_and("tools:\n  kali: { image: alpine, publish: 9000 }\n"),
        "tools.kali.publish",
        "which `port`",
    );
}

#[test]
fn tool_publish_taken_by_a_machine() {
    let yaml = format!(
        "{}tools:\n  kali: {{ image: alpine, port: 80, publish: 8080 }}\n",
        with_machine("{ networks: { lab: 5 }, ports: { web: { port: 80, publish: 8080 } }, app: { image: nginx } }")
    );
    assert_problem(&yaml, "tools.kali.publish", "already published by a machine");
}

#[test]
fn more_than_three_tools() {
    let tools = "tools:\n  a: { image: x }\n  b: { image: x }\n  c: { image: x }\n  d: { image: x }\n";
    assert_problem(&with_web_and(tools), "tools", "at most 3 tools");
}

// Existing machines.

fn external(fields: &str) -> String {
    format!(
        "{}overrides:\n  external: {{ machines: {{ m: {{ ssh: {{ {fields} }} }} }} }}\n",
        with_machine(VM)
    )
}

#[test]
fn external_address_with_a_space() {
    assert_problem(&external("address: 'a b'"), "overrides.external.machines.m.ssh", "reachable from here");
}

#[test]
fn external_port_0() {
    assert_problem(&external("address: 10.0.0.5, port: 0"), "overrides.external.machines.m.ssh", "1 to 65535");
}

#[test]
fn external_user_with_an_at() {
    assert_problem(&external("address: 10.0.0.5, user: 'a@b'"), "overrides.external.machines.m.ssh", "a user name");
}

// The message.

#[test]
fn message_placeholder_not_a_path() {
    assert_problem(
        &with_web_and("message: 'at {{ machines..web }}'\n"),
        "message",
        "isn't a placeholder Isoloom knows",
    );
    assert_problem(
        &with_web_and("message: 'at {{ machines.db.addresses.lab }}'\n"),
        "message",
        "no machine named `db`",
    );
}

// Checks.

#[test]
fn script_check_without_a_path() {
    assert_problem(&with_web_and("checks: ['']\n"), "checks[0]", "give the script's path");
}

#[test]
fn check_with_a_blank_name() {
    assert_problem(&with_check("{ name: ' ', tcp: 'web:80' }"), "checks[0].name", "give the check a name");
}

#[test]
fn check_from_an_appliance() {
    let yaml = format!(
        "{}checks:\n  - {{ from: r1, tcp: 'web:80' }}\n",
        with_machines(&[&format!("web: {WEB}"), &format!("r1: {{ networks: {{ lab: 6 }}, app: {{ {IOL} }} }}")])
    );
    assert_problem(&yaml, "checks[0].from", "is a network appliance");
}

#[test]
fn check_from_windows() {
    let yaml = format!(
        "{}checks:\n  - {{ from: win, tcp: 'web:80' }}\n",
        with_machines(&[
            "web: { networks: { lab: 5 }, os: debian@12, app: { steps: [{ run: p.sh }] } }",
            "win: { networks: { lab: 6 }, os: windows-server@2022, app: { steps: [{ run: p.ps1 }] } }",
        ])
    );
    assert_problem(&yaml, "checks[0].from", "runs Windows");
}

#[test]
fn http_options_on_a_tcp_check() {
    assert_problem(&with_check("{ tcp: 'web:80', method: POST }"), "checks[0]", "go with `http`");
}

#[test]
fn http_options_on_a_blocked_check() {
    assert_problem(
        &with_check("{ http: 'http://web/', contains: hi, expect: blocked }"),
        "checks[0]",
        "sends nothing to look at",
    );
}

#[test]
fn http_method_not_a_method() {
    assert_problem(
        &with_check("{ http: 'http://web/', method: 'G T' }"),
        "checks[0].method",
        "isn't an HTTP method",
    );
}

#[test]
fn exec_without_a_command() {
    assert_problem(&with_check("{ from: web, exec: ' ' }"), "checks[0].exec", "give a command to run");
}

#[test]
fn exec_expecting_a_status() {
    assert_problem(
        &with_check("{ from: web, exec: id, expect: 200 }"),
        "checks[0].expect",
        "the text the output must contain",
    );
}

#[test]
fn script_check_with_expect() {
    assert_problem(&with_check("{ script: checks/a.sh, expect: open }"), "checks[0].expect", "exiting 0");
}

// Files.

/// A fresh project folder under the test target's scratch space.
fn project(name: &str) -> std::io::Result<PathBuf> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("validate-files").join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The problems of `yaml` with the project paths it names checked in `dir`.
fn file_problems(yaml: &str, dir: &Path) -> Vec<String> {
    let found = match syntax::parse(yaml) {
        Ok(spec) => Lab::new(spec).problems_with_files(dir),
        Err(problems) => problems,
    };
    found.iter().map(Problem::line).collect()
}

#[test]
fn path_outside_the_project() {
    let dir = project("outside").unwrap();
    let found = file_problems(&with_machine("{ networks: { lab: 5 }, app: { build: ../web } }"), &dir);
    assert_eq!(found, ["machines.m.app.build: `../web` must be a path inside the project folder"]);
}

#[test]
fn cloud_module_without_tf_files() {
    let dir = project("no-tf").unwrap();
    std::fs::create_dir_all(dir.join("tf")).unwrap();
    let found = file_problems(&cloud("  terraform: tf\n"), &dir);
    assert_eq!(found, ["cloud.terraform: `tf` holds no .tf file: it must be a Terraform root module"]);
}
