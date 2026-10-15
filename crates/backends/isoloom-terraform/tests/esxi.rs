//! The ESXi module of a few examples, rendered from their plan, as snapshots; its determinism,
//! and its refusals read from the plan without rendering. Validated with `terraform validate`,
//! not run on a real host.

pub mod testing;

use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::EsxiBackend;
use testing::{Outcome, example, file};

/// Example `name`'s plan on ESXi.
fn env(name: &str) -> Outcome<Environment> {
    Ok(testing::planned(&example(name)?, Target::Esxi))
}

/// The module of `env`.
fn module(env: &Environment) -> Outcome<String> {
    Ok(file(&EsxiBackend.render(env)?, "esxi/main.tf")?.to_owned())
}

/// The module of example `name`.
fn esxi(name: &str) -> Outcome<String> {
    module(&env(name)?)
}

#[test]
fn esxi_hello_stack() -> Outcome {
    insta::assert_snapshot!(esxi("hello-stack")?);
    Ok(())
}

#[test]
fn esxi_segmented() -> Outcome {
    insta::assert_snapshot!(esxi("segmented")?);
    Ok(())
}

#[test]
fn esxi_ansible_pair() -> Outcome {
    insta::assert_snapshot!(esxi("ansible-pair")?);
    Ok(())
}

#[test]
fn the_module_is_the_same_each_time() -> Outcome {
    for name in ["hello-stack", "segmented", "ansible-pair"] {
        let e = env(name)?;
        assert_eq!(EsxiBackend.render(&e), EsxiBackend.render(&e), "{name}");
    }
    Ok(())
}

#[test]
fn refusals_come_from_the_plan() -> Outcome {
    let tpm = EsxiBackend.refusal(&env("tuned-web")?).expect("refused");
    assert!(tpm.contains("virtual TPM"), "{tpm}");
    let gateway = EsxiBackend.refusal(&env("edge-firewall")?).expect("refused");
    assert!(gateway.contains("gateway"), "{gateway}");
    for name in ["hello-stack", "windows-hello", "dual-stack", "shared-files"] {
        assert_eq!(EsxiBackend.refusal(&env(name)?), None, "{name}");
    }
    Ok(())
}

#[test]
fn esxi_windows_hello() -> Outcome {
    insta::assert_snapshot!(esxi("windows-hello")?);
    Ok(())
}

#[test]
fn esxi_shared_files() -> Outcome {
    insta::assert_snapshot!(esxi("shared-files")?);
    Ok(())
}

/// `virtiofs` is Proxmox's: ESXi has no folder of the host to share into a VM, so it ignores the
/// setting and shares the volume over NFS from its owner, as every other target does.
#[test]
fn virtiofs_is_ignored() -> Outcome {
    let mut spec = example("shared-files")?.for_target(Target::Esxi);
    let plain = Environment::build(&spec, Target::Esxi, None);
    spec.volumes.get_mut("shared").unwrap().virtiofs = Some(isoloom_spec::model::Virtiofs::default());
    assert_eq!(isoloom_spec::validate(&spec), vec![]);
    let with = Environment::build(&spec, Target::Esxi, None);
    assert_eq!(EsxiBackend.refusal(&with), None);
    assert_eq!(EsxiBackend.render(&with), EsxiBackend.render(&plain));
    Ok(())
}

#[test]
fn esxi_dual_stack() -> Outcome {
    insta::assert_snapshot!(esxi("dual-stack")?);
    Ok(())
}

/// What no example has on ESXi: a machine's own firmware, a network's MTU, bridged networks
/// (on a NIC of their own, tagged, and on the host's own vSwitch by DHCP), an existing port
/// group, secrets, and data disks, one of them raw.
const LAB: &str = "
version: 1
name: lab
secrets: { db-password: { env: DB_PASSWORD } }
networks:
  lan: { cidr: 10.30.0.0/24, mtu: 9000 }
  office: { cidr: 192.168.10.0/24, bridged: { interface: vmnic1, vlan: 20 } }
  home: { cidr: 192.168.1.0/24, bridged: { dhcp: true, default_route: true } }
  corp: { cidr: 10.50.0.0/24 }
machines:
  web:
    networks: { lan: 10, office: 10, home: 0, corp: 5 }
    files: [{ path: /run/secrets/db-password, secret: db-password }]
    disks: { data: { size_gb: 20, mount: /data }, scratch: { size_gb: 5, fs: raw } }
    os: debian@12
    app: { steps: [{ run: s.sh }] }
    hardware: { boot: { firmware: uefi } }
  db:
    networks: { lan: 20 }
    os: ubuntu@24.04
    app: { steps: [{ run: db.sh }] }
    hardware: { boot: { firmware: bios } }
overrides:
  esxi:
    networks: { corp: { external: corp-net } }
";

/// The plan of `text`, a valid spec, on ESXi.
fn lab(text: &str) -> Outcome<Environment> {
    testing::plan(text, Target::Esxi)
}

#[test]
fn esxi_lab() -> Outcome {
    let tf = module(&lab(LAB)?)?;
    insta::assert_snapshot!(tf);
    Ok(())
}

#[test]
fn a_boot_esxi_has_no_setting_for_is_refused() -> Outcome {
    for (boot, field) in [
        ("{ order: [network, hd] }", "order"),
        ("{ kernel: /boot/vmlinuz }", "kernel"),
        ("{ firmware: uefi, firmware_format: raw }", "firmware_format"),
    ] {
        let text = LAB.replace("hardware: { boot: { firmware: bios } }", &format!("hardware: {{ boot: {boot} }}"));
        let why = EsxiBackend.refusal(&lab(&text)?).expect("refused");
        assert!(why.contains(&format!("`hardware.boot.{field}`")), "{why}");
    }
    let order = LAB.replace("hardware: { boot: { firmware: bios } }", "hardware: { boot: { order: [hd] } }");
    assert_eq!(EsxiBackend.refusal(&lab(&order)?), None);
    Ok(())
}

#[test]
fn more_disks_than_slots_are_refused() -> Outcome {
    let disks: Vec<String> = (0..15).map(|i| format!("d{i}: {{ size_gb: 1, fs: raw }}")).collect();
    let text = LAB.replace(
        "disks: { data: { size_gb: 20, mount: /data }, scratch: { size_gb: 5, fs: raw } }",
        &format!("disks: {{ {} }}", disks.join(", ")),
    );
    let why = EsxiBackend.refusal(&lab(&text)?).expect("refused");
    assert!(why.contains("at most 14 data disks"), "{why}");
    Ok(())
}
