//! The ESXi module of a few examples, rendered from their plan, as snapshots; its determinism,
//! and its refusals read from the plan without rendering. Validated with `terraform validate`,
//! not run on a real host.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::path::Path;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::esxi::EsxiBackend;

fn env(example: &str) -> Environment {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(example);
    let spec = isoloom_spec::load(&dir).expect("the example loads");
    assert!(isoloom_spec::validate(&spec).is_empty(), "the example is valid");
    Environment::build(&spec, Target::Esxi, None)
}

fn esxi(example: &str) -> String {
    EsxiBackend
        .render(&env(example))
        .expect("the backend renders the example")
        .into_iter()
        .find(|f| f.path.ends_with("esxi/main.tf"))
        .expect("the module")
        .contents
}

#[test]
fn esxi_hello_stack() {
    insta::assert_snapshot!(esxi("hello-stack"));
}

#[test]
fn esxi_segmented() {
    insta::assert_snapshot!(esxi("segmented"));
}

#[test]
fn esxi_ansible_pair() {
    insta::assert_snapshot!(esxi("ansible-pair"));
}

#[test]
fn the_module_is_the_same_each_time() {
    for example in ["hello-stack", "segmented", "ansible-pair"] {
        let e = env(example);
        assert_eq!(EsxiBackend.render(&e), EsxiBackend.render(&e), "{example}");
    }
}

#[test]
fn refusals_come_from_the_plan() {
    let tpm = EsxiBackend.refusal(&env("tuned-web")).expect("refused");
    assert!(tpm.contains("virtual TPM"), "{tpm}");
    let gateway = EsxiBackend.refusal(&env("edge-firewall")).expect("refused");
    assert!(gateway.contains("gateway"), "{gateway}");
    for example in ["hello-stack", "windows-hello", "dual-stack", "shared-files"] {
        assert_eq!(EsxiBackend.refusal(&env(example)), None, "{example}");
    }
}

#[test]
fn esxi_windows_hello() {
    insta::assert_snapshot!(esxi("windows-hello"));
}

#[test]
fn esxi_shared_files() {
    insta::assert_snapshot!(esxi("shared-files"));
}

/// `virtiofs` is Proxmox's: ESXi has no folder of the host to share into a VM, so it ignores the
/// setting and shares the volume over NFS from its owner, as every other target does.
#[test]
fn virtiofs_is_ignored() {
    let mut spec = isoloom_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shared-files")).unwrap();
    let plain = Environment::build(&spec, Target::Esxi, None);
    spec.volumes.get_mut("shared").unwrap().virtiofs = Some(isoloom_spec::model::Virtiofs::default());
    assert_eq!(isoloom_spec::validate(&spec), vec![]);
    let with = Environment::build(&spec, Target::Esxi, None);
    assert_eq!(EsxiBackend.refusal(&with), None);
    assert_eq!(EsxiBackend.render(&with), EsxiBackend.render(&plain));
}

#[test]
fn esxi_dual_stack() {
    insta::assert_snapshot!(esxi("dual-stack"));
}

/// What no example has on ESXi: a machine's own firmware, a network's MTU, bridged networks
/// (on a NIC of their own, tagged, and on the host's own vSwitch by DHCP), an existing port
/// group, secrets, and data disks, one of them raw.
pub const LAB: &str = "
version: 1
name: lab
secrets: { db-password: { env: DB_PASSWORD } }
networks:
  lan: { cidr: 10.30.0.0/24, mtu: 9000 }
  office: { cidr: 192.168.10.0/24, bridged: { interface: vmnic1, vlan: 20 } }
  home: { cidr: 192.168.1.0/24, bridged: { dhcp: true, default_route: true } }
  corp: { cidr: 10.50.0.0/24, external: corp-net }
machines:
  web:
    networks: { lan: 10, office: 10, home: 0, corp: 5 }
    secrets: [db-password]
    disks: { data: { size_gb: 20, mount: /data }, scratch: { size_gb: 5, fs: raw } }
    vm: { os: debian@12, provision: [s.sh], boot: { firmware: uefi } }
  db:
    networks: { lan: 20 }
    vm: { os: ubuntu@24.04, provision: [db.sh], boot: { firmware: bios } }
";

fn lab(text: &str) -> Environment {
    let spec = isoloom_spec::parse(text).unwrap();
    assert_eq!(isoloom_spec::validate(&spec), vec![]);
    Environment::build(&spec, Target::Esxi, None)
}

#[test]
fn esxi_lab() {
    let files = EsxiBackend.render(&lab(LAB)).unwrap();
    let tf = &files.iter().find(|f| f.path.ends_with("esxi/main.tf")).unwrap().contents;
    insta::assert_snapshot!(tf);
}

#[test]
fn a_boot_esxi_has_no_setting_for_is_refused() {
    for (boot, field) in [
        ("{ order: [network, hd] }", "order"),
        ("{ kernel: /boot/vmlinuz }", "kernel"),
        ("{ firmware: uefi, firmware_format: raw }", "firmware_format"),
    ] {
        let text = LAB.replace("provision: [db.sh], boot: { firmware: bios }", &format!("provision: [db.sh], boot: {boot}"));
        let why = EsxiBackend.refusal(&lab(&text)).expect("refused");
        assert!(why.contains(&format!("`vm.boot.{field}`")), "{why}");
    }
    let order = LAB.replace("provision: [db.sh], boot: { firmware: bios }", "provision: [db.sh], boot: { order: [hd] }");
    assert_eq!(EsxiBackend.refusal(&lab(&order)), None);
}

#[test]
fn more_disks_than_slots_are_refused() {
    let disks: Vec<String> = (0..15).map(|i| format!("d{i}: {{ size_gb: 1, fs: raw }}")).collect();
    let text = LAB.replace(
        "disks: { data: { size_gb: 20, mount: /data }, scratch: { size_gb: 5, fs: raw } }",
        &format!("disks: {{ {} }}", disks.join(", ")),
    );
    let why = EsxiBackend.refusal(&lab(&text)).expect("refused");
    assert!(why.contains("at most 14 data disks"), "{why}");
}
