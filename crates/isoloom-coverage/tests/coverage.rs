//! The formats' tables stay honest: every key, setting and resource type of a format has a row,
//! what the examples' generated files write is marked as written, and docs/COVERAGE.md is what
//! `isoloom coverage --markdown` prints. The spec's side has its own tests (spec_table.rs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use isoloom_coverage::markdown;
use isoloom_spec::load;
use isoloom_targets::generate;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn docs_coverage_is_up_to_date() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/COVERAGE.md");
    let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(
        on_disk == markdown(),
        "docs/COVERAGE.md is stale; run `cargo run -q -- coverage --markdown > docs/COVERAGE.md`"
    );
}

// From the formats' side.

use isoloom_coverage::{Support, formats, vagrant};
use isoloom_spec::model::Target;

#[path = "support/vagrantfile.rs"]
mod vagrantfile;

/// Checks a format's rows against what the generator really writes: every key written has a
/// row marked as written, and every row marked as written is written by some example.
fn written_matches(rows: &[(String, Support)], written: &BTreeSet<String>, format: &str) {
    // Every mismatch at once: one run says everything to reclassify.
    let mut wrong = Vec::new();
    for key in written {
        match rows.iter().find(|(k, _)| k == key) {
            None => wrong.push(format!("Isoloom writes `{key}`, which has no coverage row")),
            Some((_, s)) if !s.written() => wrong.push(format!("Isoloom writes `{key}`, but its row says {}", s.kind())),
            Some(_) => {}
        }
    }
    for (key, s) in rows {
        if s.written() && !written.contains(key) {
            wrong.push(format!("`{key}` is marked as written, but no example's output has it"));
        }
    }
    assert!(wrong.is_empty(), "{format}:\n{}", wrong.join("\n"));
}

/// A machine with its own box source and boot, which no example needs (a lab about the boot
/// itself, served on libvirt).
const OWN_BOOT: &str = "
version: 1
name: own-boot
networks: { lan: { cidr: 10.30.0.0/24 } }
machines:
  pxe:
    networks: { lan: 10 }
    vm:
      os: debian-12
      provision: [setup.sh]
      image: { vagrant: lab/debian-12, box_url: 'https://boxes.example/debian-12.box', box_server_url: 'https://boxes.example' }
      boot:
        kernel: /var/lib/isoloom/vmlinuz
        initrd: /var/lib/isoloom/initrd.img
        cmdline: console=ttyS0
        dtb: /var/lib/isoloom/board.dtb
        loader: /usr/share/OVMF/OVMF_CODE.fd
        nvram: /var/lib/isoloom/pxe_VARS.fd
        firmware_format: raw
        order: [network, hd]
        network: { tftp_root: /srv/tftp, file: pxelinux.0, server: 10.30.0.1 }
";

/// Specs for what no example exercises: a VM on the host's LAN, at a fixed address and by DHCP
/// with the LAN's default route (`public_network`); a Windows 11 machine (the TPM its OS
/// expects), a Windows box reached over OpenSSH (`winssh`) that requires a TPM, and a machine
/// running VMs of its own (every provider's nested virtualization switch).
const VAGRANT_FIXTURES: &[&str] = &[
    "
version: 1
name: home-lan
networks:
  lan: { cidr: 10.20.0.0/24 }
  home: { cidr: 192.168.1.0/24, bridged: { interface: 'en0: Wi-Fi' } }
  wifi: { cidr: 192.168.2.0/24, bridged: { dhcp: true, default_route: true } }
machines:
  a: { networks: { lan: 10, home: 50, wifi: 0 }, vm: { os: debian-12, provision: [setup.sh] } }
",
    "
version: 1
name: windows-guests
networks: { lab: { cidr: 192.168.58.0/24 } }
machines:
  ws01:
    networks: { lab: 21 }
    vm: { os: windows-11, provision: [setup.ps1] }
  srv01:
    networks: { lab: 10 }
    vm: { os: windows-server-2022, provision: [setup.ps1], image: { winssh: true }, tpm: true }
",
    "
version: 1
name: nested-fixture
networks: { lab: { cidr: 10.9.0.0/24 } }
machines:
  hv: { networks: { lab: 10 }, nested_virtualization: true, vm: { os: debian-12, provision: [setup.sh] } }
",
];

/// Every Vagrantfile the generators write for the examples: the vagrant output (every provider
/// is a block of the same file), docker-vm and hybrid, generated now rather than read from the
/// committed copies, so the check follows the generators; then the fixtures' (and
/// [`OWN_BOOT`]) on `vagrant`.
fn vagrantfiles() -> Vec<String> {
    let mut found = Vec::new();
    for target in [Target::Vagrant, Target::DockerVm, Target::Hybrid] {
        let before = found.len();
        for entry in std::fs::read_dir(examples()).unwrap() {
            let dir = entry.unwrap().path();
            let Ok(spec) = load(&dir) else { continue };
            let Ok(files) = generate(&spec, target) else { continue };
            found.extend(files.into_iter().filter(|f| f.path.ends_with("/Vagrantfile")).map(|f| f.contents));
        }
        assert!(found.len() > before, "no example generates a Vagrantfile for {target:?}");
    }
    for fixture in VAGRANT_FIXTURES {
        let files = generate(&isoloom_spec::parse(fixture).unwrap(), Target::Vagrant).unwrap();
        found.extend(files.into_iter().filter(|f| f.path.ends_with("/Vagrantfile")).map(|f| f.contents));
    }
    let fixture = isoloom_spec::parse(OWN_BOOT).unwrap();
    assert_eq!(isoloom_spec::validate(&fixture), vec![], "the own-boot fixture");
    let files = generate(&fixture, Target::Vagrant).unwrap();
    found.extend(files.into_iter().filter(|f| f.path.ends_with("/Vagrantfile")).map(|f| f.contents));
    found
}

#[test]
fn vagrant_written_settings_match_the_generated_files() {
    let rows: Vec<(String, Support)> = vagrant::formats().into_iter().flat_map(|f| f.rows).collect();
    let common = |key: &str| rows.iter().any(|(k, _)| *k == format!("config.vm.provision *: {key}"));
    let mut written = BTreeSet::new();
    for text in vagrantfiles() {
        written.extend(vagrantfile::written(&text, &common));
    }
    written_matches(&rows, &written, "Vagrant");
}

#[test]
fn a_vagrantfile_reads_back_into_its_settings() {
    let text = r#"
Vagrant.configure("2") do |config|
  config.vm.synced_folder ".", "/vagrant", disabled: true
  config.vm.define "a" do |m|
    m.vm.network "forwarded_port", guest: 53, host: 10053, protocol: "udp" # a: comment
    m.vm.disk :disk, size: "10GB"
    m.vm.provision "shell", name: "x", run: "never", inline: <<~'SH'
      m.vm.nothing = "inside the script"
    SH
    m.vm.provider "qemu" do |v, o|
      v.smp = "cpus=2"
      if x
        o.winrm.timeout = 1
      end
      v.memory = "1G"
    end
    m.trigger.before :up, type: :action do |t|
    end
  end
end
"#;
    let common = |k: &str| ["name", "run"].contains(&k);
    let got: Vec<String> = vagrantfile::written(text, &common).into_iter().collect();
    let want = [
        "config.trigger",
        "config.vm.define",
        "config.vm.disk",
        "config.vm.disk: size",
        "config.vm.disk: type",
        "config.vm.network",
        "config.vm.network forwarded_port",
        "config.vm.network forwarded_port: guest",
        "config.vm.network forwarded_port: host",
        "config.vm.network forwarded_port: protocol",
        "config.vm.provider",
        "config.vm.provision",
        "config.vm.provision *: name",
        "config.vm.provision *: run",
        "config.vm.provision shell",
        "config.vm.provision shell: inline",
        "config.vm.synced_folder",
        "config.vm.synced_folder: disabled",
        "config.winrm",
        "config.winrm.timeout",
        "provider qemu: memory",
        "provider qemu: smp",
    ];
    assert_eq!(got, want);
}
#[test]
fn every_feature_is_classified() {
    for f in formats() {
        let unclassified: Vec<_> = f.rows.iter().filter(|(_, s)| *s == Support::Unclassified).map(|(k, _)| k.as_str()).collect();
        assert!(unclassified.is_empty(), "{}: classify these in coverage/: {unclassified:?}", f.name);
        let mut keys = BTreeSet::new();
        for (k, _) in &f.rows {
            assert!(keys.insert(k), "{}: `{k}` is listed twice", f.name);
        }
    }
}
