//! The ESXi module of a few examples, rendered from their plan, as snapshots; its determinism,
//! and its refusals read from the plan without rendering. Validated with `terraform validate`,
//! not run on a real host.

use crate::testing::{Outcome, example, file, written, written_files};
use isoloom_plan::Environment;
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::EsxiBackend;
use isoloom_test_support::{hcl_blocks, hcl_body, hcl_map, hcl_names, hcl_nested, hcl_resource, hcl_text, hcl_values};

/// Example `name`'s plan on ESXi.
fn env(name: &str) -> Outcome<Environment> {
    crate::testing::planned(&example(name)?, Target::Esxi)
}

/// The module of `env`.
fn module(env: &Environment) -> Outcome<String> {
    Ok(file(&EsxiBackend.render(env)?, "esxi/main.tf")?.to_owned())
}

/// The module of example `name`.
fn esxi(name: &str) -> Outcome<String> {
    module(&env(name)?)
}

/// The modules a reviewer reads as files.
#[test]
fn the_esxi_module_of_the_examples() -> Outcome {
    for name in ["hello-stack", "segmented", "ansible-pair", "windows-hello", "shared-files", "dual-stack"] {
        insta::assert_snapshot!(format!("esxi_{}", name.replace('-', "_")), esxi(name)?);
    }
    Ok(())
}

/// The router holds the sealed ruleset from its first boot, and the module's seal applies that
/// very file over SSH once the machines are set up, within the apply: `internet: false` networks
/// lose the internet on ESXi, as on Proxmox. An environment with nothing to seal has neither.
#[test]
fn the_router_holds_the_sealed_ruleset_the_seal_applies() -> Outcome {
    let e = env("segmented")?;
    let sealed = module(&e)?;
    let files = written_files(&sealed, "esxi_guest", "isoloom_router");
    let file = |path: &str| -> Outcome<String> { Ok(written(&files, path)?.get("content").cloned().unwrap_or_default()) };
    // The `internet: false` networks' traffic from elsewhere dropped, and only the others
    // forwarded out, in the sealed ruleset alone.
    let closed: Vec<String> = e.networks.values().filter(|n| !n.internet).map(|n| n.cidr.to_string()).collect();
    assert!(!closed.is_empty(), "segmented has an internet: false network");
    let drop = format!("ip daddr {{ {} }} drop", closed.join(", "));
    assert!(
        file("/etc/nftables.sealed.nft")?.lines().any(|l| l.trim() == drop),
        "the sealed ruleset drops {closed:?}"
    );
    assert!(!file("/etc/nftables.conf")?.contains(&drop), "the first boot's ruleset doesn't");
    // The seal applies that very file and keeps it across reboots.
    let seal = hcl_resource(&sealed, "terraform_data", "isoloom_seal");
    let inline: Vec<String> = hcl_nested(&seal, "provisioner").iter().flat_map(|p| hcl_values(p.body(), "inline")).collect();
    assert_eq!(
        inline,
        ["sudo sh -c 'nft -f /etc/nftables.sealed.nft && cp /etc/nftables.sealed.nft /etc/nftables.conf'"]
    );
    // Nothing to seal: neither.
    let open = module(&env("ansible-pair")?)?;
    assert!(!hcl_names(&hcl_body(&open), "resource", "terraform_data").contains(&"isoloom_seal".to_owned()));
    let router = written_files(&open, "esxi_guest", "isoloom_router");
    assert!(written(&router, "/etc/nftables.sealed.nft").is_err(), "{router:?}");
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
    assert_eq!(
        EsxiBackend.refusal(&env("tuned-web")?).as_deref(),
        Some("machine `web` asks for a virtual TPM (`hardware.tpm: required`), which this target doesn't give (`isoloom matrix` lists the targets that do)")
    );
    assert_eq!(
        EsxiBackend.refusal(&env("edge-firewall")?).as_deref(),
        Some("Isoloom routes no network through a `gateway` machine on ESXi")
    );
    for name in ["hello-stack", "windows-hello", "dual-stack", "shared-files"] {
        assert_eq!(EsxiBackend.refusal(&env(name)?), None, "{name}");
    }
    Ok(())
}

/// `virtiofs` is Proxmox's: ESXi has no folder of the host to share into a VM, so it ignores the
/// setting and shares the volume over NFS from its owner, as every other target does.
#[test]
fn virtiofs_is_ignored() -> Outcome {
    let lab = example("shared-files")?;
    let mut spec = lab.for_target(Target::Esxi);
    let plain = lab.plan(&spec, Target::Esxi, None)?;
    spec.volumes.get_mut("shared").ok_or("no shared volume")?.virtiofs = Some(isoloom_spec::model::Virtiofs::default());
    assert_eq!(isoloom_spec::validate(&spec), vec![]);
    let with = lab.plan(&spec, Target::Esxi, None)?;
    assert_eq!(EsxiBackend.refusal(&with), None);
    assert_eq!(EsxiBackend.render(&with), EsxiBackend.render(&plain));
    Ok(())
}

/// What no example has on ESXi: a machine's own firmware, a network's MTU, bridged networks
/// (on a NIC of their own, tagged, and on the host's own vSwitch by DHCP), an existing port
/// group, and data disks, one of them raw. (Secrets are refused: nothing delivers them to a machine
/// behind the router.)
const LAB: &str = "
version: 1
name: lab
networks:
  lan: { cidr: 10.30.0.0/24, mtu: 9000 }
  office: { cidr: 192.168.10.0/24, bridged: { interface: vmnic1, vlan: 20 } }
  home: { cidr: 192.168.1.0/24, bridged: { dhcp: true, default_route: true } }
  corp: { cidr: 10.50.0.0/24 }
machines:
  web:
    networks: { lan: 10, office: 10, home: 0, corp: 5 }
    files: [{ path: /etc/motd, content: web }]
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
    crate::testing::plan(text, Target::Esxi)
}

#[test]
fn firmware_mtu_bridged_networks_an_existing_port_group_and_data_disks_on_esxi() -> Outcome {
    insta::assert_snapshot!("esxi_lab", module(&lab(LAB)?)?);
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

#[test]
fn the_default_ova_is_pinned_to_a_dated_release() -> Outcome {
    // The `images` default points at a dated OVA, never the floating `current`, so a generated
    // environment keeps booting the same OS build; its SHA-256 is in the variable's description,
    // since the provider checks none.
    let tf = hcl_body(&esxi("hello-stack")?);
    let images = hcl_blocks(&tf, "variable", &["images"]);
    let images = images.first().ok_or("no images variable")?.body();
    assert_eq!(
        hcl_map(images, "default"),
        [(
            "ubuntu-24.04".to_owned(),
            "https://cloud-images.ubuntu.com/noble/20260926/noble-server-cloudimg-amd64.ova".to_owned()
        )]
    );
    let description = hcl_text(images, "description");
    assert!(
        description.contains("ubuntu-24.04 513a22ebe3982b9387b038f8a2a6dad1af980d4623dca03511312e55ba620b1a"),
        "{description}"
    );
    Ok(())
}
