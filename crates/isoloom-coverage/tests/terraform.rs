//! The Terraform tables stay honest: every resource, data source, argument, nested block and
//! provider setting a generator writes for a committed example is a row marked as written, and
//! every row marked as written is written by some example. The tables themselves come from the
//! providers' schemas, so a written key with no row is an argument the provider doesn't have.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hcl::{Block, Body, Structure};
use isoloom_coverage::{Format, formats, written_problems};
use isoloom_spec::model::Target;

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Every Terraform module the generators write, under an example's `.isoloom/`: Proxmox,
/// docker-vm's Proxmox module, and each cloud's cloud-vm and cloud-docker modules.
const MODULES: &[&str] = &[
    "proxmox",
    "esxi",
    "docker-vm/proxmox",
    "cloud-vm/aws",
    "cloud-vm/azure",
    "cloud-vm/gcp",
    "cloud-vm/digitalocean",
    "cloud-vm/linode",
    "cloud-vm/oci",
    "cloud-docker/aws",
    "cloud-docker/azure",
    "cloud-docker/gcp",
    "cloud-docker/digitalocean",
    "cloud-docker/linode",
    "cloud-docker/oci",
];

/// Terraform's own arguments, valid in every resource: not the provider's, so not rows.
const META: &[&str] = &["count", "for_each", "depends_on", "provider", "lifecycle", "provisioner", "connection"];

/// The `.tf` files of one module across the examples; each module must exist in some example.
fn module_files(module: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(examples()).unwrap() {
        let dir = entry.unwrap().path().join(".isoloom").join(module);
        let Ok(files) = std::fs::read_dir(&dir) else { continue };
        for f in files {
            let p = f.unwrap().path();
            if p.extension().is_some_and(|x| x == "tf") {
                found.push(std::fs::read_to_string(p).unwrap());
            }
        }
    }
    assert!(!found.is_empty(), "no example has a .isoloom/{module} module");
    found
}

/// The keys a body writes, under `prefix`: attributes, and nested blocks with their contents
/// (a `dynamic "x"` block counts as `x`, with its `content`).
fn walk(body: &Body, prefix: &str, top: bool, out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Structure::Attribute(a) if !(top && META.contains(&a.key.as_str())) => {
                out.insert(format!("{prefix}{}", a.key.as_str()));
            }
            Structure::Block(b) if !(top && META.contains(&b.identifier.as_str())) => nested(b, prefix, out),
            _ => {}
        }
    }
}

fn nested(b: &Block, prefix: &str, out: &mut BTreeSet<String>) {
    let (name, body) = if b.identifier.as_str() == "dynamic" {
        let content = b
            .body
            .blocks()
            .find(|c| c.identifier.as_str() == "content")
            .expect("a dynamic block has content");
        (b.labels[0].as_str().to_string(), &content.body)
    } else {
        (b.identifier.as_str().to_string(), &b.body)
    };
    out.insert(format!("{prefix}{name}"));
    walk(body, &format!("{prefix}{name}."), false, out);
}

/// Every key a module file writes, as the tables name them: `resource T`, `data T`,
/// `argument T.path`, `argument data.T.path` and `provider P: path`.
pub fn written_keys(text: &str) -> BTreeSet<String> {
    let body = hcl::parse(text).unwrap();
    let mut out = BTreeSet::new();
    for b in body.blocks() {
        let label = |i: usize| b.labels[i].as_str().to_string();
        match b.identifier.as_str() {
            "resource" => {
                out.insert(format!("resource {}", label(0)));
                walk(&b.body, &format!("argument {}.", label(0)), true, &mut out);
            }
            "data" => {
                out.insert(format!("data {}", label(0)));
                walk(&b.body, &format!("argument data.{}.", label(0)), true, &mut out);
            }
            "provider" => walk(&b.body, &format!("provider {}: ", label(0)), true, &mut out),
            _ => {}
        }
    }
    out
}

/// The Terraform table a key belongs to: the one with a row for its resource type, data
/// source or provider.
fn owner<'a>(tables: &'a [Format], key: &str) -> Option<&'a Format> {
    let anchor = if let Some(rest) = key.strip_prefix("argument data.") {
        format!("data {}", rest.split('.').next().unwrap())
    } else if let Some(rest) = key.strip_prefix("argument ") {
        format!("resource {}", rest.split('.').next().unwrap())
    } else if key.starts_with("provider ") {
        return tables
            .iter()
            .find(|f| f.rows.iter().any(|(k, _)| k.split(": ").next() == key.split(": ").next()));
    } else {
        key.to_string()
    };
    tables.iter().find(|f| f.rows.iter().any(|(k, _)| *k == anchor))
}

/// Machines with what no example needs: images and boots of their own (every cloud's own image,
/// a cloud image's URL on Proxmox, a Windows machine's own Azure image, a Proxmox template
/// booted through UEFI in an order of its own), GPUs (passed through on Proxmox by PCI address
/// and by resource mapping; a cloud's GPU instance type), VMs running VMs of their own, and a
/// port published over UDP on a cloud VM (Oracle Cloud's per-machine security group rule).
const OWN_IMAGES: &[(&str, Target)] = &[
    (
        "web: { networks: { lan: 10 }, vm: { os: debian-12, provision: [s.sh], image: { proxmox: 'https://images.example/d.qcow2', aws: ami-0abc, azure: /subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/d, gcp: projects/p/global/images/d, digitalocean: '4242', linode: private/42, oci: ocid1.image.oc1..d } } }",
        Target::CloudVm,
    ),
    (
        "dc: { networks: { lan: 10 }, vm: { os: windows-server-2022, provision: [s.ps1], image: { azure: /subscriptions/s/resourceGroups/g/providers/Microsoft.Compute/images/w } } }",
        Target::CloudVm,
    ),
    (
        "web: { networks: { lan: 10 }, vm: { os: debian-12, provision: [s.sh], image: { proxmox: '9000' }, boot: { firmware: uefi, firmware_format: raw, order: [hd, network] } } }",
        Target::Proxmox,
    ),
    (
        "gpu: { networks: { lan: 10 }, vm: { os: debian-12, provision: [s.sh] }, gpus: { ids: ['0000:01:00.0', rtx] } }",
        Target::Proxmox,
    ),
    (
        "gpu: { networks: { lan: 10 }, vm: { os: debian-12, provision: [s.sh] }, gpus: 1 }",
        Target::CloudVm,
    ),
    (
        "hv: { networks: { lan: 10 }, vm: { os: debian-12, provision: [s.sh] }, nested_virtualization: true }",
        Target::CloudVm,
    ),
    (
        "dns: { networks: { lan: 53 }, services: [{ port: 53, protocol: udp, publish: 10053 }], vm: { os: ubuntu-24.04, provision: [s.sh] } }",
        Target::CloudVm,
    ),
];

/// The modules generated for [`OWN_IMAGES`].
fn own_image_modules() -> Vec<String> {
    let mut out = Vec::new();
    for (machine, target) in OWN_IMAGES {
        let text = format!("version: 1\nname: own\nnetworks: {{ lan: {{ cidr: 10.30.0.0/24 }} }}\nmachines:\n  {machine}\n");
        let spec = isoloom_spec::parse(&text).unwrap();
        assert_eq!(isoloom_spec::validate(&spec), vec![], "{machine}");
        let files = isoloom_targets::generate(&spec, *target).unwrap();
        out.extend(
            files
                .into_iter()
                .filter(|f| Path::new(&f.path).extension().is_some_and(|x| x == "tf"))
                .map(|f| f.contents),
        );
    }
    out
}

/// Whole environments with what no example needs, on Proxmox (not run on a real host): a
/// network's MTU and a machine's rate limit, and machines run as LXC containers.
const PROXMOX_FIXTURES: &[&str] = &[
    "version: 1\nname: links\nnetworks:\n  lan: { cidr: 10.30.0.0/24, mtu: 1400 }\nmachines:\n  web: { networks: { lan: 10 }, rate_limit: 10mbit, vm: { os: debian-12, provision: [s.sh] } }\n",
    "version: 1\nname: lxc\nnetworks:\n  lan: { cidr: 10.30.0.0/24, mtu: 1400 }\n  office: { cidr: 10.31.0.0/16, vlans: { 10: { cidr: 10.31.10.0/24 } } }\nmachines:\n  web: { networks: { lan: 10, office.vlan10: 10 }, rate_limit: 10mbit, environment: { MODE: lab }, docker: { image: 'nginx:1.27', idle: true, lxc: true } }\n",
];

/// The modules generated for [`PROXMOX_FIXTURES`].
fn proxmox_fixture_modules() -> Vec<String> {
    PROXMOX_FIXTURES
        .iter()
        .flat_map(|text| {
            let spec = isoloom_spec::parse(text).unwrap();
            assert_eq!(isoloom_spec::validate(&spec), vec![], "{text}");
            isoloom_targets::generate(&spec, Target::Proxmox).unwrap()
        })
        .filter(|f| Path::new(&f.path).extension().is_some_and(|x| x == "tf"))
        .map(|f| f.contents)
        .collect()
}

/// Every key written by any module (the examples', then the [`OWN_IMAGES`] fixtures'), grouped
/// by the table it belongs to.
fn written_by_table(tables: &[Format]) -> BTreeMap<&'static str, BTreeSet<String>> {
    let mut by: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let examples = MODULES
        .iter()
        .flat_map(|m| module_files(m).into_iter().map(move |t| (format!(".isoloom/{m}"), t)));
    let fixtures = own_image_modules()
        .into_iter()
        .chain(proxmox_fixture_modules())
        .map(|t| ("the fixtures".to_string(), t));
    for (module, text) in examples.chain(fixtures) {
        for key in written_keys(&text) {
            let table = owner(tables, &key).unwrap_or_else(|| panic!("{module}: writes `{key}`, which no Terraform table lists"));
            by.entry(table.name).or_default().insert(key);
        }
    }
    by
}

fn terraform_tables() -> Vec<Format> {
    formats().into_iter().filter(|f| f.name.starts_with("Terraform: ")).collect()
}

#[test]
fn terraform_written_keys_match_the_generated_modules() {
    let tables = terraform_tables();
    let written = written_by_table(&tables);
    let problems: Vec<String> = tables
        .iter()
        .flat_map(|f| written_problems(f, &written.get(f.name).cloned().unwrap_or_default()))
        .collect();
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_classification_rule_names_something() {
    for t in isoloom_coverage::terraform::TABLES {
        let stale = t.stale_rules();
        assert!(stale.is_empty(), "{}: rules that name nothing in the provider's schema: {stale:?}", t.name);
    }
}

#[test]
#[ignore = "a tool: prints every key the modules write"]
fn print_written_keys() {
    let mut all = BTreeSet::new();
    for module in MODULES {
        for text in module_files(module) {
            all.extend(written_keys(&text));
        }
    }
    for k in all {
        println!("{k}");
    }
}
