//! The coverage table stays honest: every field of the format has a row, every ✓ or ◐ names
//! an example that uses the field and generates for that output, and docs/COVERAGE.md is
//! what `isoloom coverage --markdown` prints.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use isoloom_coverage::{Output, Status, markdown, paths, table};
use isoloom_spec::{load, parse};
use isoloom_targets::generate;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// A spec with every field set. Serialized back, unset fields appear too (as null), so a field
/// added to the model shows up here without editing this text.
const EVERY_FIELD: &str = r#"
version: 1
name: every-field
networks:
  out: { cidr: 10.1.0.0/24, internet: true, docker: { cidr: 10.1.0.0/24 } }
  dmz: { cidr: 10.1.1.0/24, internet: false, gateway: fw, docker: { cidr: 10.1.1.0/24 } }
  far: { cidr: 10.1.2.0/24, tc: { delay: 50ms, jitter: 5ms, loss: 0.5, rate: 10mbit } }
  office: { cidr: 10.2.0.0/16, docker: { cidr: 10.2.0.0/16 }, switch: fw, vlans: { 10: { cidr: 10.2.10.0/24, internet: false } } }
reach: [{ from: out, to: dmz, ports: [80] }]
inputs: [TOKEN]
volumes: { data: { owner: fw } }
machines:
  fw:
    networks: { out: 2, dmz: 1 }
    services: [{ port: 80, name: web, http: true, tls: true, publish: 8080, fixed: true }, { port: 4444, dormant: true }, { port: 53, protocol: udp }]
    inputs: [TOKEN]
    arch: amd64
    privileged: true
    read_only: true
    tmpfs: [/tmp, /run]
    shm_size: 256m
    dns: { servers: [10.0.0.1], search: [lab.local], domain: lab.local }
    environment: { MODE: x }
    sysctls: { net.core.somaxconn: 1024 }
    ulimits: { nofile: 1024 }
    resources: { cpus: 1, memory_mb: 512, disk_gb: 10 }
    depends_on: []
    volumes: { data: /data }
    access: false
    aliases: [api.example.test]
    docker: { image: a, init: [x.sh], idle: true, appliance: cisco-iol, config: r.cfg, firmware: f.bin }
    vm: { os: debian-12, provision: [x.sh], image: { vagrant: x/y, vagrant_version: "1", qemu: x/z, hyperv: x/h, winrm: ssl } }
    external: { address: 192.168.1.2, user: admin, port: 2222, key: ~/.ssh/lab }
  h:
    count: 2
    networks: { out: 10 }
    docker: { build: h, dockerfile: h.Dockerfile, args: { A: b } }
provision: [{ ansible: site.yml, inventory: [inv.ini], groups: { dc: [fw] }, vars: { a: b }, host_vars: { fw: { a: b } }, requirements: req.yml }]
controller: { image: { vagrant: x/y }, resources: { cpus: 1, memory_mb: 512 } }
common: { resources: { cpus: 1 } }
groups: { g: { members: [fw], arch: amd64 } }
checks: [c.sh, { name: n, from: fw, http: "http://x/", method: POST, headers: { A: b }, body: x, contains: y, follow: true, send: z, tcp: "x:1", exec: e, script: s.sh, expect: 200, wait: 1 }]
targets: [docker]
message: hello
cloud: { provider: aws, terraform: tf, vars: { region: us-east-1 }, outputs: { site: url }, hourly_usd: 0.1, before: [k.sh], env: { AWS_REGION: us-east-1 }, apply_retries: 1, requires: [jq] }
tools: { shell: {}, viewer: { image: x, command: [serve], port: 80, publish: 9000 } }
"#;

#[test]
fn every_field_of_the_format_has_a_row_and_every_row_is_a_field() {
    parse(EVERY_FIELD).expect("parses");
    // The document as written: `count`, `common`, `groups` and `vlans` are folded away by parsing.
    let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(EVERY_FIELD).unwrap();
    let fields: BTreeSet<String> = paths(&raw).into_iter().collect();
    let rows: BTreeSet<String> = table().iter().map(|r| r.path.to_string()).collect();
    let missing: Vec<_> = fields.difference(&rows).collect();
    let stale: Vec<_> = rows.difference(&fields).collect();
    assert!(missing.is_empty(), "fields with no coverage row: {missing:?}");
    assert!(stale.is_empty(), "coverage rows for fields that don't exist: {stale:?}");
}

#[test]
fn every_proxmox_overlay_path_is_a_row() {
    let rows: BTreeSet<&str> = table().iter().map(|r| r.path).collect();
    let unknown: Vec<_> = isoloom_coverage::spec::PROXMOX.iter().map(|(p, _)| *p).filter(|p| !rows.contains(p)).collect();
    assert!(unknown.is_empty(), "Proxmox overlay entries for fields with no row: {unknown:?}");
}

#[test]
fn every_claim_is_exercised_by_an_example() {
    for row in table() {
        for o in Output::ALL {
            let (Status::Done { proof, .. } | Status::Partial { proof, .. }) = row.status(o) else {
                continue;
            };
            let dir = examples().join(proof);
            let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(&std::fs::read_to_string(dir.join("isoloom.yml")).unwrap()).unwrap();
            assert!(
                paths(&raw).iter().any(|p| p == row.path),
                "{} on {}: example `{proof}` doesn't use the field",
                row.path,
                o.label()
            );
            let spec = load(&dir).unwrap();
            assert!(
                generate(&spec, o.target()).is_ok(),
                "{} on {}: example `{proof}` doesn't generate for it",
                row.path,
                o.label()
            );
        }
    }
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
