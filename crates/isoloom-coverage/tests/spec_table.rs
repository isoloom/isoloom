//! The spec table stays honest: every field of the format has a row, every rendering states
//! every field, every claim rests on an example (or the field's probe) that generates for the
//! output, and no backend refuses what its own rendering claims.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // helpers outside #[test] functions too: a failing test is the report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use isoloom_coverage::spec::run::{self, Outcome};
use isoloom_coverage::spec::status::PROBE;
use isoloom_coverage::spec::{Cell, FIELDS, Family, Output, claim, parts, paths, stated, table};
use isoloom_coverage::{Status, markdown};
use isoloom_spec::{load, parse};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn every_field_of_the_format_has_a_row_and_every_row_is_a_field() {
    parse(EVERY_FIELD).expect("parses");
    // The document as written: `count`, `common`, `groups` and `vlans` are folded away by parsing.
    let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(EVERY_FIELD).unwrap();
    let fields: BTreeSet<String> = paths(&raw).into_iter().collect();
    let rows: BTreeSet<String> = FIELDS.iter().map(|p| (*p).to_string()).collect();
    let missing: Vec<_> = fields.difference(&rows).collect();
    let stale: Vec<_> = rows.difference(&fields).collect();
    assert!(missing.is_empty(), "fields with no coverage row: {missing:?}");
    assert!(stale.is_empty(), "coverage rows for fields that don't exist: {stale:?}");
}

#[test]
fn every_rendering_states_every_field_once() {
    for f in Family::ALL {
        let mut seen = BTreeSet::new();
        for (path, _) in parts(f).iter().flat_map(|p| p.iter()) {
            assert!(FIELDS.contains(path), "{f:?} states `{path}`, which is no field");
            assert!(seen.insert(*path), "{f:?} states `{path}` twice");
        }
        let unstated: Vec<_> = FIELDS.iter().filter(|p| stated(f, p).is_none()).collect();
        assert!(unstated.is_empty(), "{f:?} doesn't state {unstated:?}");
    }
}

/// Whether example `name` uses the field and generates for `output`.
fn example_shows(name: &str, path: &str, output: Output) -> Result<(), String> {
    let dir = examples().join(name);
    let text = std::fs::read_to_string(dir.join("isoloom.yml")).map_err(|e| format!("example `{name}`: {e}"))?;
    let raw: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    if !paths(&raw).iter().any(|p| p == path) {
        return Err(format!("example `{name}` doesn't use the field"));
    }
    match run::render(&load(&dir).unwrap(), output) {
        Some(Ok(())) => Ok(()),
        Some(Err(why)) => Err(format!("example `{name}` is refused: {why}")),
        None => Err(format!("example `{name}` doesn't run on it")),
    }
}

/// Whether some example uses the field and generates for `output`.
fn any_example_shows(path: &str, output: Output) -> bool {
    std::fs::read_dir(examples())
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().into_string().ok())
        .any(|name| example_shows(&name, path, output).is_ok())
}

/// The outputs that can prove a primary output's claim: the output itself, or for the cloud VM
/// modules any cloud, since one cloud's claim (IPv6 on Azure and Oracle Cloud) may be one the
/// reference cloud refuses with its own reason.
fn provers(o: Output) -> Vec<Output> {
    match o {
        Output::CloudVm(_) => Output::all().into_iter().filter(|c| matches!(c, Output::CloudVm(_))).collect(),
        other => vec![other],
    }
}

/// Whether `proof` (an example, or the field's probe) generates `path` on `o`.
fn proven(path: &str, proof: &str, o: Output) -> Result<(), String> {
    if proof == PROBE {
        match run::outcome(path, o) {
            Outcome::Renders => Ok(()),
            other => Err(format!("its probe: {other:?}")),
        }
    } else {
        example_shows(proof, path, o)
    }
}

#[test]
fn every_claim_rests_on_an_example_or_its_probe() {
    let mut wrong = Vec::new();
    for o in Output::all().into_iter().filter(|o| o.is_primary()) {
        for path in FIELDS {
            let Some(proof) = claim(path, o).and_then(Status::proof) else { continue };
            let results: Vec<Result<(), String>> = provers(o).into_iter().map(|p| proven(path, proof, p)).collect();
            if results.iter().any(Result::is_ok) {
                continue;
            }
            if let Some(Err(e)) = results.first() {
                wrong.push(format!("{path} on {}: {e}", o.label()));
            }
        }
    }
    assert!(wrong.is_empty(), "claims without proof:\n{}", wrong.join("\n"));
}

#[test]
fn an_inherited_claim_generates_on_its_output() {
    let mut wrong = Vec::new();
    for row in table() {
        for (o, cell) in &row.cells {
            let Cell::Is(s @ (Status::Done { .. } | Status::Partial { .. })) = cell else {
                continue;
            };
            if o.is_primary() || run::outcome(row.path, *o) == Outcome::Renders || any_example_shows(row.path, *o) {
                continue;
            }
            wrong.push(format!(
                "{} on {}: {} with neither its probe nor an example generating",
                row.path,
                o.label(),
                s.symbol()
            ));
        }
    }
    assert!(wrong.is_empty(), "inherited claims nothing shows:\n{}", wrong.join("\n"));
}

#[test]
fn every_field_that_applies_somewhere_has_a_valid_probe_using_it() {
    let mut wrong = Vec::new();
    for path in FIELDS {
        for o in Output::all() {
            if !claim(path, o).is_some_and(Status::applies) {
                continue;
            }
            match run::documents(path, o) {
                Err(e) => wrong.push(e),
                Ok(docs) => {
                    if !docs.iter().any(|d| paths(d).iter().any(|p| p == path)) {
                        wrong.push(format!("`{path}`'s probe for {} doesn't use it", o.label()));
                    }
                }
            }
            if let Outcome::NoProbe(e) = run::outcome(path, o) {
                wrong.push(format!("`{path}` for {}: {e}", o.label()));
            }
        }
    }
    wrong.dedup();
    assert!(wrong.is_empty(), "probes:\n{}", wrong.join("\n"));
}

#[test]
fn the_page_carries_the_spec_table_with_a_column_per_output() {
    let md = markdown();
    let header: String = Output::all().iter().map(|o| [" ", o.label(), " |"].concat()).collect();
    assert!(md.contains(&format!("| Field |{header}")), "no spec table on the page");
    assert!(md.contains("### Refusals"));
}

/// A spec with every field set. Serialized back, unset fields appear too (as null), so a field
/// added to the model shows up here without editing this text.
const EVERY_FIELD: &str = r#"
version: 1
name: every-field
networks:
  out: { cidr: 10.1.0.0/24, internet: true, docker: { cidr: 10.1.0.0/24 }, ipv6: { cidr: "fd00:1::/64", addresses: auto, only: false } }
  dmz: { cidr: 10.1.1.0/24, internet: false, gateway: fw, docker: { cidr: 10.1.1.0/24 } }
  far: { cidr: 10.1.2.0/24, mtu: 1400, tc: { delay: 50ms, jitter: 5ms, loss: 0.5, rate: 10mbit } }
  office: { cidr: 10.2.0.0/16, docker: { cidr: 10.2.0.0/16 }, switch: fw, vlans: { 10: { cidr: 10.2.10.0/24, internet: false } } }
  corp: { cidr: 10.3.0.0/24, external: corp-net }
  home: { cidr: 10.4.0.0/24, bridged: { interface: en0, dhcp: true, default_route: true, vlan: 20 } }
reach: [{ from: out, to: dmz, ports: [80] }]
dns: { private: true, domain: every.internal }
inputs: [TOKEN]
secrets: { token: { env: TOKEN } }
volumes: { data: { owner: fw, persistent: true, external: lab-data, virtiofs: { mapping: lab-data, cache: never, direct_io: true, xattr: true, acl: true } } }
machines:
  fw:
    networks: { out: 2, dmz: 1 }
    ports: [{ port: 80, name: web, http: true, tls: true, publish: 8080, fixed: true }, { port: 4444, dormant: true }, { port: 53, protocol: udp }]
    inputs: [TOKEN]
    secrets: [token, { source: token, target: /etc/app/token, uid: 0, gid: 0, mode: 0400 }]
    arch: amd64
    rate_limit: 10mbit
    privileged: true
    nested_virtualization: true
    gpus: { count: 1, ids: [], driver: nvidia, capabilities: [compute], options: { a: b } }
    devices: [{ host: /dev/fuse, guest: /dev/fuse, permissions: rw }]
    read_only: true
    tmpfs: [/tmp, /run]
    shm_size: 256m
    dns: { servers: [10.0.0.1], search: [lab.local], domain: lab.local, options: ["ndots:2"] }
    environment: { MODE: x }
    sysctls: { net.core.somaxconn: 1024 }
    ulimits: { nofile: 1024 }
    resources: { cpus: 1, memory_mb: 512, disk_gb: 10 }
    depends_on: []
    volumes: { data: /data }
    access: false
    aliases: [api.example.test]
    os: debian@12
    docker: { image: a, init: [x.sh], idle: true, appliance: cisco-iol, config: r.cfg, firmware: f.bin, lxc: false }
    vm:
      os: debian-12
      provision: [x.sh]
      image: { vagrant: x/y, vagrant_version: "1", qemu: x/z, hyperv: x/h, winrm: ssl, winssh: true, box_url: x.box, box_server_url: "https://b", proxmox: "9000", aws: ami-1, azure: /subscriptions/s, gcp: projects/p/global/images/i, digitalocean: "1", linode: private/1, oci: ocid1.image.x }
      tpm: true
      boot: { firmware: uefi, kernel: /k, initrd: /i, cmdline: c, dtb: /d, loader: /l, nvram: /n, firmware_format: raw, order: [network], network: { tftp_root: /t, file: f, server: 10.0.0.1 } }
    external: { address: 192.168.1.2, user: admin, port: 2222, key: ~/.ssh/lab }
  h:
    count: 2
    networks: { out: [10, 20] }
    docker: { build: h, dockerfile: h.Dockerfile, target: lab, args: { A: b } }
    files: [{ path: /etc/a, content: x, uid: 1, gid: 1, mode: "0600" }, { path: /etc/b, source: b }]
    command: [serve]
    working_dir: /srv
    before: [b.sh]
    healthcheck: { test: t, http: { port: 80, path: / }, interval: 1s, timeout: 1s, retries: 1, start_period: 1s }
    volumes: { data: { path: /data, subpath: h } }
    tmpfs: [{ path: /run, size: 64m, mode: "1777" }]
    disks: { d: { size_gb: 1, fs: ext4, mount: /d } }
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
