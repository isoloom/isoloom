//! The ESXi module on small plans: a vSwitch and a port group per network, a VM per machine set
//! up through the router, and the refusals.

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use super::EsxiBackend;

fn env(spec: &str) -> Environment {
    crate::testing::environment(spec, Target::Esxi)
}

/// The plan of a spec validation refuses, to see the backend refuse it too.
fn unchecked(spec: &str) -> Environment {
    crate::testing::planned(spec, Target::Esxi)
}

const PAIR: &str = "
version: 1
name: pair
networks:
  front: { cidr: 10.20.1.0/24 }
  back: { cidr: 10.20.2.0/24 }
machines:
  web:
    networks: { front: 10, back: 10 }
    ports: { web: { port: 80, publish: 8080 } }
    os: ubuntu@24.04
    app: { steps: [{ run: web.sh }] }
  db:
    networks: { back: 20 }
    os: debian@12
    app: { steps: [{ run: db.sh }] }
";

fn module(spec: &str) -> String {
    let files = EsxiBackend.render(&env(spec)).unwrap();
    files.into_iter().find(|f| f.path.ends_with("esxi/main.tf")).unwrap().contents
}

#[test]
fn the_password_comes_from_the_environment_never_a_command_line() {
    let tf = module(PAIR);
    let header: String = tf.lines().take_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n");
    assert!(header.contains("export TF_VAR_esxi_password=<password>"), "{header}");
    assert!(!header.contains("-var esxi_password"), "{header}");
}

#[test]
fn a_vswitch_and_a_port_group_per_network() {
    let body = isoloom_test_support::hcl_body(&module(PAIR));
    assert_eq!(isoloom_test_support::hcl_names(&body, "resource", "esxi_vswitch"), ["front", "back"]);
    assert_eq!(isoloom_test_support::hcl_names(&body, "resource", "esxi_portgroup"), ["front", "back"]);
    assert_eq!(
        isoloom_test_support::hcl_text(isoloom_test_support::hcl_one(&body, "esxi_portgroup", "back"), "vswitch"),
        "esxi_vswitch.back.name"
    );
}

#[test]
fn a_vm_per_machine_set_up_through_the_router() {
    let tf = module(PAIR);
    let body = isoloom_test_support::hcl_body(&tf);
    let mut guests = isoloom_test_support::hcl_names(&body, "resource", "esxi_guest");
    guests.sort();
    assert_eq!(guests, ["db", "isoloom_router", "web"]);
    // web's set-up goes through the router.
    let web = isoloom_test_support::hcl_one(&body, "terraform_data", "web");
    let bastions: Vec<String> = isoloom_test_support::hcl_nested(web, "connection")
        .iter()
        .map(|c| isoloom_test_support::hcl_text(c.body(), "bastion_host"))
        .collect();
    isoloom_test_support::assert_every!(bastions, |b: &String| b == "local.router_address");
    let locals = isoloom_test_support::hcl_blocks(&body, "locals", &[]);
    let router = locals
        .iter()
        .find_map(|l| isoloom_test_support::hcl_attribute(l.body(), "router_address"))
        .map(ToString::to_string);
    assert_eq!(router.as_deref(), Some("esxi_guest.isoloom_router.ip_address"));
    // Debian has no default OVA: the variable says it must be given.
    let images = isoloom_test_support::hcl_blocks(&body, "variable", &["images"]);
    let images = images.first().expect("the images variable").body();
    assert!(
        isoloom_test_support::hcl_literals(images)
            .iter()
            .any(|l| l.contains("images needs an OVA for each of: debian-12, ubuntu-24.04.")),
        "{tf}"
    );
    assert!(
        isoloom_test_support::hcl_literals(images)
            .iter()
            .any(|l| l.ends_with("noble-server-cloudimg-amd64.ova")),
        "{tf}"
    );
}

#[test]
fn windows_and_ipv6_are_hosted_within_limits() {
    let windows = PAIR.replace("os: debian@12", "os: windows-server@2022").replace("db.sh", "db.ps1");
    assert_eq!(EsxiBackend.refusal(&env(&windows)), None);
    let body = isoloom_test_support::hcl_body(&module(&windows));
    assert_eq!(
        isoloom_test_support::hcl_text(isoloom_test_support::hcl_one(&body, "esxi_guest", "db"), "guestos"),
        "windows2019srvnext-64"
    );
    assert_eq!(isoloom_test_support::hcl_names(&body, "resource", "random_password"), ["windows"]);
    assert!(
        !isoloom_test_support::hcl_names(&body, "resource", "terraform_data").contains(&"db".to_owned()),
        "cloudbase-init sets it up"
    );
    let shell = PAIR.replace("os: debian@12", "os: windows-server@2022");
    let why = EsxiBackend.refusal(&unchecked(&shell)).unwrap();
    assert!(why.contains("PowerShell scripts"), "{why}");
    let v6 = PAIR.replace("back: { cidr: 10.20.2.0/24 }", "back: { cidr: 10.20.2.0/24, ipv6: { cidr: \"fd00:20::/64\" } }");
    assert_eq!(EsxiBackend.refusal(&env(&v6)), None);
    let router = isoloom_test_support::hcl_resource(&module(&v6), "esxi_guest", "isoloom_router");
    let router_addresses = isoloom_test_support::hcl_literals(&router);
    assert!(router_addresses.iter().any(|l| l == "fd00:20::254/64"), "the router's IPv6 address");
    let kali = PAIR.replace("os: debian@12", "os: kali");
    let no_image = EsxiBackend.refusal(&env(&kali)).unwrap();
    assert!(no_image.contains("no ESXi image for `kali`"), "{no_image}");
    assert_eq!(EsxiBackend.refusal(&env(PAIR)), None);
}

/// A Linux machine's secret or input as a file comes from the module's variable, written by
/// cloud-init from its guestinfo user data and read by its `Secrets` step; a Windows machine's is refused (its set-up
/// has no step that takes it).
#[test]
fn a_linux_machines_secrets_come_from_the_modules_variables_and_a_windows_ones_are_refused() {
    let secret = ("secrets: { token: { env: TOKEN } }", "{ path: /etc/token, secret: token }");
    let input = ("inputs: [TOKEN]", "{ path: /etc/token, input: TOKEN }");
    for (top, file) in [secret, input] {
        let spec = format!(
            "version: 1\nname: s\n{top}\nnetworks: {{ lan: {{ cidr: 10.20.1.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, os: debian@12, files: [{file}] }}\n"
        );
        assert_eq!(EsxiBackend.refusal(&env(&spec)), None, "{spec}");
        let set_up = isoloom_test_support::hcl_literals(&isoloom_test_support::hcl_resource(&module(&spec), "terraform_data", "web"));
        assert!(
            set_up
                .iter()
                .any(|c| c.contains(". /var/lib/isoloom/secrets.env && set +a && rm -f /var/lib/isoloom/secrets.env")),
            "{set_up:?}"
        );
        let windows = spec.replace("os: debian@12", "os: windows-server@2022").replace("/etc/token", "C:\\\\token");
        let why = crate::server::secrets_refusal(&env(&windows)).unwrap_or_default();
        assert!(why.contains("gets `secrets`"), "{why}");
    }
}
