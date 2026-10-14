//! Probe specs: for each field, the smallest spec that uses it. Every backend is asked to
//! render each probe, so a column says "refused" from the backend's own refusal (its
//! capabilities against the plan's requirements, then its own limits) instead of from a table
//! someone has to keep in step. A probe is a fragment merged into a baseline: maps merge, a list
//! or a value replaces, and `null` removes a key.

/// One machine every machine target can run (a container, a VM and an existing host at once),
/// on one network, serving one port.
pub const BASELINE: &str = "
version: 1
name: probe
networks:
  lan: { cidr: 10.20.0.0/24 }
machines:
  a:
    networks: { lan: 10 }
    services: [{ port: 80 }]
    docker: { image: nginx }
    vm: { os: debian-12, provision: [setup.sh] }
    external: { address: 192.0.2.10 }
";

/// Cloud services, which are a target of their own: no machines.
pub const CLOUD_BASELINE: &str = "
version: 1
name: probe
cloud: { provider: aws, terraform: tf }
";

/// A second machine like the first, for fields that relate two machines.
const B: &str = "b: { networks: { lan: 11 }, docker: { image: nginx }, vm: { os: debian-12, provision: [setup.sh] }, external: { address: 192.0.2.11 }";

/// A second network, for the second machine.
const DMZ: &str = "networks: { dmz: { cidr: 10.20.1.0/24 } }";

/// Each field's probe over [`BASELINE`]; a field the baseline uses needs none.
pub fn probe(path: &str) -> Option<String> {
    let fragment = match path {
        "networks.*.tc.delay" => "networks: { lan: { tc: { delay: 50ms } } }".to_string(),
        "networks.*.tc.jitter" => "networks: { lan: { tc: { delay: 50ms, jitter: 5ms } } }".to_string(),
        "networks.*.tc.loss" => "networks: { lan: { tc: { loss: 1 } } }".to_string(),
        "networks.*.tc.rate" => "networks: { lan: { tc: { rate: 10mbit } } }".to_string(),
        "networks.*.mtu" => "networks: { lan: { mtu: 1400 } }".to_string(),
        "machines.*.rate_limit" => "machines: { a: { rate_limit: 10mbit } }".to_string(),
        "machines.*.docker.lxc" => "machines: { a: { vm: null, external: null, docker: { lxc: true } } }".to_string(),
        "networks.*.docker.cidr" => "networks: { lan: { docker: { cidr: 10.20.0.0/24 } } }".to_string(),
        "networks.*.internet" => "networks: { lan: { internet: false } }".to_string(),
        "networks.*.ipv6.cidr" => "networks: { lan: { ipv6: { cidr: 'fd00:20::/64' } } }".to_string(),
        "networks.*.ipv6.addresses" => "networks: { lan: { ipv6: { cidr: 'fd00:20::/64', addresses: auto } } }".to_string(),
        "networks.*.ipv6.only" => "networks: { lan: { internet: false, ipv6: { cidr: 'fd00:20::/64', only: true } } }".to_string(),
        "networks.*.gateway" => "{ networks: { dmz: { cidr: 10.20.1.0/24, gateway: a } }, machines: { a: { networks: { dmz: 1 } } } }".to_string(),
        "networks.*.vlans.*.cidr" | "networks.*.vlans.*.internet" => vlans(path),
        "networks.*.external" => "{ networks: { corp: { cidr: 10.77.0.0/24, external: corp-net } }, machines: { a: { networks: { corp: 10 } } } }".to_string(),
        "networks.*.bridged.interface" | "networks.*.bridged.dhcp" | "networks.*.bridged.default_route" | "networks.*.bridged.vlan" => bridged(path),
        "networks.*.switch" => SWITCH.to_string(),
        "reach[].from" | "reach[].to" | "reach[].ports" => {
            let b = B.replace("lan: 11", "dmz: 11");
            format!("{{ {DMZ}, machines: {{ {b} }} }}, reach: [{{ from: lan, to: dmz, ports: [80] }}] }}")
        }
        "inputs" => "inputs: [TOKEN]".to_string(),
        "machines.*.inputs" => "{ inputs: [TOKEN], machines: { a: { inputs: [TOKEN] } } }".to_string(),
        "secrets" | "machines.*.secrets" => {
            "{ secrets: { token: { env: TOKEN } }, machines: { a: { secrets: [token, { source: token, target: /etc/app/token, mode: 0400 }] } } }".to_string()
        }
        "machines.*.depends_on" => format!("machines: {{ {B}, depends_on: [a] }} }}"),
        "machines.*.access" => format!("machines: {{ {B}, access: true }} }}"),
        "volumes.*.owner" => {
            format!("{{ volumes: {{ data: {{ owner: a }} }}, machines: {{ a: {{ volumes: {{ data: /data }} }}, {B}, volumes: {{ data: /data }} }} }} }}")
        }
        "provision[].ansible"
        | "provision[].inventory"
        | "provision[].groups"
        | "provision[].vars"
        | "provision[].host_vars"
        | "provision[].requirements"
        | "controller" => provision(path),
        "machines.*.docker.appliance" => appliance(""),
        "machines.*.docker.config" => appliance(", config: r1.cfg"),
        "machines.*.docker.firmware" => "machines: { r: { networks: { lan: 2 }, docker: { appliance: cisco-dynamips, firmware: c7200.bin } } }".to_string(),
        "checks[]" => "checks: [check.sh, { name: probe, from: a, tcp: 'a:80' }]".to_string(),
        "groups" => "groups: { web: { members: [a], arch: amd64 } }".to_string(),
        "tools" => "tools: { shell: {} }".to_string(),
        _ => return machine(path).map(|m| format!("machines: {{ a: {m} }}")).or_else(|| check(path)),
    };
    Some(fragment)
}

/// A router appliance beside the machine (an appliance is a container only).
fn appliance(extra: &str) -> String {
    format!("machines: {{ r: {{ networks: {{ lan: 2 }}, docker: {{ image: vrnetlab/cisco_iol, appliance: cisco-iol{extra} }} }} }}")
}

/// The host's LAN, bridged, with the machine on it after its own network: at a fixed address,
/// or by DHCP (`0`).
fn bridged(path: &str) -> String {
    let (bridged, octet) = match path {
        "networks.*.bridged.dhcp" => ("dhcp: true", 0),
        "networks.*.bridged.default_route" => ("dhcp: true, default_route: true", 0),
        "networks.*.bridged.vlan" => ("vlan: 20", 50),
        _ => ("interface: en0", 50),
    };
    format!("{{ networks: {{ home: {{ cidr: 192.168.1.0/24, bridged: {{ {bridged} }} }} }}, machines: {{ a: {{ networks: {{ home: {octet} }} }} }} }}")
}

/// A LAN with a VLAN the machine is on.
fn vlans(path: &str) -> String {
    let internet = if path.ends_with("internet") { ", internet: false" } else { "" };
    format!(
        "{{ networks: {{ office: {{ cidr: 10.30.0.0/16, vlans: {{ 10: {{ cidr: 10.30.10.0/24{internet} }} }} }} }}, machines: {{ a: {{ networks: {{ lan: null, office.vlan10: 10 }} }} }} }}"
    )
}

/// A LAN switched by an appliance, with a container on one of its VLANs.
const SWITCH: &str = "{ networks: { office: { cidr: 10.30.0.0/16, switch: sw, vlans: { 10: { cidr: 10.30.10.0/24 } } } }, machines: { \
    a: { networks: { office.vlan10: 10 } }, sw: { docker: { image: vrnetlab/cisco_iol, appliance: cisco-iol-l2 } } } }";

/// Environment-level provisioning, with the part `path` names.
fn provision(path: &str) -> String {
    let extra = match path {
        "provision[].inventory" => ", inventory: [extra.ini]",
        "provision[].groups" => ", groups: { web: [a] }",
        "provision[].vars" => ", vars: { mode: lab }",
        "provision[].host_vars" => ", host_vars: { a: { mode: lab } }",
        "provision[].requirements" => ", requirements: requirements.yml",
        _ => "",
    };
    let controller = if path == "controller" {
        ", controller: { resources: { cpus: 1 } }"
    } else {
        ""
    };
    format!("{{ provision: [{{ ansible: site.yml{extra} }}]{controller} }}")
}

/// The first machine with the field `path` names set, as a flow mapping.
fn machine(path: &str) -> Option<&'static str> {
    Some(match path.strip_prefix("machines.*.")? {
        "services[].publish" => "{ services: [{ port: 80, publish: 8080 }] }",
        "services[].fixed" => "{ services: [{ port: 80, publish: 8080, fixed: true }] }",
        "services[].dormant" => "{ services: [{ port: 80 }, { port: 4444, dormant: true }] }",
        "services[].protocol" => "{ services: [{ port: 80 }, { port: 53, protocol: udp }] }",
        "environment" => "{ environment: { MODE: lab } }",
        "sysctls" => "{ sysctls: { net.core.somaxconn: 1024 } }",
        "ulimits" => "{ ulimits: { nofile: 1024 } }",
        "aliases" => "{ aliases: [www.probe.test] }",
        "arch" => "{ arch: arm64 }",
        "privileged" => "{ privileged: true }",
        "read_only" => "{ read_only: true }",
        "tmpfs" => "{ tmpfs: [/run] }",
        "files[].path" | "files[].content" => "{ files: [{ path: /etc/app.ini, content: x }] }",
        "files[].source" => "{ files: [{ path: /etc/app.ini, source: app.ini }] }",
        "files[].uid" => "{ files: [{ path: /etc/app.ini, content: x, uid: 1000 }] }",
        "files[].gid" => "{ files: [{ path: /etc/app.ini, content: x, gid: 1000 }] }",
        "files[].mode" => "{ files: [{ path: /etc/app.ini, content: x, mode: '0600' }] }",
        "shm_size" => "{ shm_size: 256m }",
        "dns.servers" => "{ dns: { servers: [10.20.0.53] } }",
        "dns.search" => "{ dns: { servers: [10.20.0.53], search: [probe.test] } }",
        "dns.domain" => "{ dns: { domain: probe.test } }",
        "dns.options" => "{ dns: { options: ['ndots:2'] } }",
        "resources.cpus" => "{ resources: { cpus: 2 } }",
        "resources.memory_mb" => "{ resources: { memory_mb: 1024 } }",
        "resources.disk_gb" => "{ resources: { disk_gb: 20 } }",
        "volumes" => "{ volumes: { data: /data } }",
        "docker.build" => "{ docker: { image: null, build: app } }",
        "docker.dockerfile" => "{ docker: { image: null, build: app, dockerfile: app.Dockerfile } }",
        "docker.target" => "{ docker: { image: null, build: app, target: lab } }",
        "docker.args" => "{ docker: { image: null, build: app, args: { MODE: lab } } }",
        "docker.init" => "{ docker: { init: [init.sh] } }",
        "docker.idle" => "{ docker: { idle: true } }",
        "vm.image.vagrant" => "{ vm: { image: { vagrant: debian/bookworm64 } } }",
        "vm.image.vagrant_version" => "{ vm: { image: { vagrant: debian/bookworm64, vagrant_version: '12.20240503.1' } } }",
        "vm.image.qemu" => "{ vm: { image: { qemu: generic/debian12 } } }",
        "vm.image.hyperv" => "{ vm: { image: { hyperv: generic/debian12 } } }",
        "vm.image.winrm" => "{ vm: { os: windows-server-2022, provision: [setup.ps1], image: { winrm: plaintext } } }",
        "vm.image.winssh" => "{ vm: { os: windows-server-2022, provision: [setup.ps1], image: { winssh: true } } }",
        "vm.tpm" => "{ vm: { tpm: true } }",
        "command" => "{ command: [nginx, -g, 'daemon off;'] }",
        "working_dir" => "{ working_dir: /srv }",
        "before" => "{ command: [nginx, -g, 'daemon off;'], before: [before.sh] }",
        "healthcheck.test" => "{ healthcheck: { test: 'true' } }",
        "healthcheck.http.port" | "healthcheck.http.path" => "{ healthcheck: { http: { port: 80, path: / } } }",
        "healthcheck.interval" | "healthcheck.timeout" | "healthcheck.retries" | "healthcheck.start_period" => {
            "{ healthcheck: { test: 'true', interval: 2s, timeout: 1s, retries: 3, start_period: 0s } }"
        }
        "external.user" => "{ external: { user: admin } }",
        "external.port" => "{ external: { port: 2222 } }",
        "external.key" => "{ external: { key: ~/.ssh/lab } }",
        other => return hardware(other).or_else(|| super::families::own_image::probe(other)),
    })
}

/// A machine's hardware field set: GPUs, the host's devices, nested virtualization.
fn hardware(field: &str) -> Option<&'static str> {
    Some(match field {
        "nested_virtualization" => "{ nested_virtualization: true }",
        "gpus.count" => "{ gpus: { count: 1 } }",
        "gpus.ids" => "{ gpus: { ids: ['0000:01:00.0'] } }",
        "gpus.driver" => "{ gpus: { count: 1, driver: nvidia } }",
        "gpus.capabilities" => "{ gpus: { count: 1, capabilities: [compute, utility] } }",
        "gpus.options" => "{ gpus: { count: 1, options: { virtualization: 'false' } } }",
        "devices[].host" => "{ devices: [{ host: /dev/fuse }] }",
        "devices[].guest" => "{ devices: [{ host: /dev/fuse, guest: /dev/fuse0 }] }",
        "devices[].permissions" => "{ devices: [{ host: /dev/fuse, permissions: rw }] }",
        _ => return None,
    })
}

/// A check from the first machine using the field `path` names.
fn check(path: &str) -> Option<String> {
    let probe = match path.strip_prefix("checks[]")? {
        ".name" | ".from" | ".http" | ".expect" | ".wait" => "http: 'http://a/', expect: 200, wait: 10",
        ".tcp" => "tcp: 'a:80'",
        ".method" => "http: 'http://a/', method: POST",
        ".headers" => "http: 'http://a/', headers: { Accept: text/html }",
        ".body" => "http: 'http://a/', method: POST, body: hello",
        ".contains" => "http: 'http://a/', contains: welcome",
        ".follow" => "http: 'http://a/', follow: true",
        ".send" => "tcp: 'a:80', send: hello, contains: welcome",
        ".exec" => "exec: 'true'",
        ".script" => "script: check.sh",
        _ => return None,
    };
    Some(format!("checks: [{{ name: probe, from: a, {probe} }}]"))
}

/// Each field's probe over [`CLOUD_BASELINE`], for the fields cloud services use.
pub fn cloud_probe(path: &str) -> Option<String> {
    let fragment = match path {
        "inputs" => "inputs: [TOKEN]",
        "message" => "message: hello",
        "cloud.vars" => "cloud: { vars: { region: us-east-1 } }",
        "cloud.outputs" => "cloud: { outputs: { site: url } }",
        "cloud.hourly_usd" => "cloud: { hourly_usd: 0.01 }",
        "cloud.before" => "cloud: { before: [setup.sh] }",
        "cloud.env" => "cloud: { env: { AWS_REGION: us-east-1 } }",
        "cloud.apply_retries" => "cloud: { apply_retries: 2 }",
        "cloud.requires" => "cloud: { requires: [jq] }",
        "checks[]" => "checks: [check.sh, { name: probe, http: 'https://example.com/' }]",
        _ => {
            return check(path).map(|c| {
                c.replace("from: a, ", "")
                    .replace("'http://a/'", "'https://example.com/'")
                    .replace("'a:80'", "'example.com:443'")
            });
        }
    };
    Some(fragment.to_string())
}
