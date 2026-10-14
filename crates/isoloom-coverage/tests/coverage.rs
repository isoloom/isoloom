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

/// Every committed output of a generator, across the examples: each file must exist in at
/// least one example, so a renamed output can't silently leave the check with nothing to read.
fn outputs(files: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for file in files {
        let before = found.len();
        for entry in std::fs::read_dir(examples()).unwrap() {
            let p = entry.unwrap().path().join(file);
            if let Ok(text) = std::fs::read_to_string(p) {
                found.push(text);
            }
        }
        assert!(found.len() > before, "no example has {file}");
    }
    found
}

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

/// Every Vagrantfile the generators write for the examples: the vagrant output (every provider
/// is a block of the same file), docker-vm and hybrid, generated now rather than read from the
/// committed copies, so the check follows the generators.
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

/// Which committed Terraform files each provider table describes.
const TERRAFORM_OUTPUTS: &[(&str, &[&str])] = &[
    ("Terraform: Proxmox", &[".isoloom/proxmox/main.tf", ".isoloom/docker-vm/proxmox/main.tf"]),
    ("Terraform: AWS", &[".isoloom/cloud-vm/aws/main.tf", ".isoloom/cloud-docker/aws/main.tf"]),
    ("Terraform: Azure", &[".isoloom/cloud-vm/azure/main.tf", ".isoloom/cloud-docker/azure/main.tf"]),
    (
        "Terraform: Google Cloud",
        &[".isoloom/cloud-vm/gcp/main.tf", ".isoloom/cloud-docker/gcp/main.tf"],
    ),
    (
        "Terraform: DigitalOcean",
        &[".isoloom/cloud-vm/digitalocean/main.tf", ".isoloom/cloud-docker/digitalocean/main.tf"],
    ),
    (
        "Terraform: Linode",
        &[".isoloom/cloud-vm/linode/main.tf", ".isoloom/cloud-docker/linode/main.tf"],
    ),
    (
        "Terraform: Oracle Cloud",
        &[".isoloom/cloud-vm/oci/main.tf", ".isoloom/cloud-docker/oci/main.tf"],
    ),
];

/// Resource types of the small utility providers (keys, passwords, files, waits) the
/// generators use beside each platform's own: they have no coverage table.
const UTILITY_RESOURCES: &[&str] = &["local_", "random_", "terraform_data", "time_", "tls_", "null_"];

#[test]
fn terraform_written_resources_match_the_generated_files() {
    let formats = formats();
    for (name, files) in TERRAFORM_OUTPUTS {
        let format = formats
            .iter()
            .find(|f| f.name == *name)
            .unwrap_or_else(|| panic!("no coverage table named {name}"));
        let mut written = BTreeSet::new();
        for text in outputs(files) {
            for line in text.lines() {
                let Some(rest) = line.trim_start().strip_prefix("resource \"") else {
                    continue;
                };
                let kind = rest.split('"').next().unwrap();
                let key = format!("resource {kind}");
                if format.rows.iter().any(|(k, _)| *k == key) {
                    written.insert(key);
                } else {
                    assert!(
                        UTILITY_RESOURCES.iter().any(|p| kind.starts_with(p)),
                        "{name}: Isoloom writes `{kind}`, which is neither one of the provider's resource types nor a utility one"
                    );
                }
            }
        }
        written_matches(&format.rows, &written, name);
    }
    // A table that claims written resources must be checked against some output.
    for f in formats.iter().filter(|f| f.name.starts_with("Terraform: ")) {
        if f.rows.iter().any(|(_, s)| s.written()) {
            assert!(
                TERRAFORM_OUTPUTS.iter().any(|(n, _)| *n == f.name),
                "{}: marks resources as written, but no generated output is checked against it",
                f.name
            );
        }
    }
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

#[test]
fn every_cloud_resource_named_exists_in_its_provider() {
    for c in isoloom_coverage::terraform::CLOUDS {
        let all = vagrant::names(c.list);
        for r in c
            .environment
            .iter()
            .chain(c.windows)
            .chain(c.generated)
            .chain(c.equivalent.iter().map(|(r, _)| r))
        {
            assert!(all.contains(r), "{}: `{r}` isn't a resource type of {}", c.name, c.source);
        }
    }
}
