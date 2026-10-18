//! A seeded generator of random but valid specs in the new syntax, for `cargo xtask fuzz-combos`.
//!
//! Each seed names one spec, built deterministically, so a failing seed reproduces its spec
//! exactly. The generator stays inside the format's rules by construction: disjoint `/24`
//! networks, host octets unique within a network, references (inputs, secrets, `depends_on`,
//! a check's `from`) only to names it has already declared, and published host ports drawn from
//! a per-spec counter so none collide. The harness asserts the spec is valid, so a spec that
//! isn't is a generator bug the fast test catches.
//!
//! It varies what Phase 6 asks it to: the machines, their OS, the `app` form (an image, a build,
//! the configuration model, or a plain OS), the ports map, files reading inputs and secrets,
//! per-target overrides, the networks and the `setup` scripts. A `user` step carries a recognis-
//! able sentinel password ([`SENTINEL_PREFIX`]), which is hashed before it reaches any file: the
//! harness proves the plaintext never leaks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// The prefix of every sentinel secret value the generator plants (a `user` step's password), so
/// the leak check and the regression corpus can both recognise one on sight.
pub(crate) const SENTINEL_PREFIX: &str = "P6FUZZ-SENTINEL-";

/// The Linux OS references the generator draws from: they have both a container and a VM form, so
/// most targets can generate them.
const LINUX_OS: [&str; 8] = [
    "debian@12",
    "debian@13",
    "ubuntu@22.04",
    "ubuntu@24.04",
    "alpine@3.19",
    "rocky@9",
    "alma@9",
    "fedora@42",
];

/// The Windows OS references the generator draws from now and then: VM targets only, so they
/// exercise the container targets' refusals (which must carry a reason).
const WINDOWS_OS: [&str; 2] = ["windows-server@2019", "windows@11"];

/// A spec the generator built: its text, and the sentinel secret values planted in it, which
/// must appear in no generated file.
pub(crate) struct GeneratedSpec {
    /// The `isoloom.yml` text.
    pub(crate) yaml: String,
    /// The sentinel secret values planted in it.
    pub(crate) secret_values: Vec<String>,
}

/// A small, deterministic pseudo-random generator (`SplitMix64`), so a seed reproduces its spec
/// without a dependency.
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    /// A generator seeded with `seed`.
    pub(crate) fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// The next 64 bits.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// An integer in `lo..=hi` (`lo` when `hi <= lo`).
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + u32::try_from(self.next() % u64::from(hi - lo + 1)).unwrap_or(0)
    }

    /// An index into `0..len` (0 when `len` is 0).
    fn below(&mut self, len: usize) -> usize {
        let len = u64::try_from(len).unwrap_or(0);
        if len == 0 {
            return 0;
        }
        usize::try_from(self.next() % len).unwrap_or(0)
    }

    /// Whether a `percent`-in-100 chance comes up.
    fn chance(&mut self, percent: u32) -> bool {
        self.range(1, 100) <= percent
    }

    /// One of `items`, or `""` for an empty slice (callers pass a non-empty one).
    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        let at = self.below(items.len());
        items.get(at).copied().unwrap_or_default()
    }
}

/// The generator's running state while it builds one spec.
struct Builder {
    rng: Rng,
    machines: Vec<String>,
    networks: Vec<String>,
    inputs: Vec<String>,
    secrets: Vec<String>,
    used: BTreeMap<String, BTreeSet<u32>>,
    ported: BTreeSet<usize>,
    host_port: u32,
    secret_values: Vec<String>,
    sentinel: u32,
}

/// The spec named by `seed`: valid by construction, varied in its machines, OS, forms, ports,
/// inputs and secrets, overrides, networks and setup.
pub(crate) fn spec(seed: u64) -> GeneratedSpec {
    let mut b = Builder {
        rng: Rng::new(seed),
        machines: Vec::new(),
        networks: Vec::new(),
        inputs: Vec::new(),
        secrets: Vec::new(),
        used: BTreeMap::new(),
        ported: BTreeSet::new(),
        host_port: 20000,
        secret_values: Vec::new(),
        sentinel: 0,
    };
    let yaml = b.build(seed);
    GeneratedSpec {
        yaml,
        secret_values: b.secret_values,
    }
}

impl Builder {
    /// Builds the whole spec text.
    fn build(&mut self, seed: u64) -> String {
        let mut out = String::new();
        writeln!(out, "version: 1\nname: fuzzlab{seed}").ok();
        self.networks(&mut out);
        self.declarations(&mut out);
        self.machines_block(&mut out);
        self.checks(&mut out);
        self.overrides(&mut out);
        out
    }

    /// The `networks` block: one to three disjoint `/24`s, each perhaps without internet.
    fn networks(&mut self, out: &mut String) {
        let count = self.rng.range(1, 3);
        let mut octets: Vec<u32> = Vec::new();
        out.push_str("networks:\n");
        for i in 0..count {
            let name = format!("n{i}");
            let octet = self.free_octet(&mut octets);
            let internet = if self.rng.chance(50) { ", internet: false" } else { "" };
            writeln!(out, "  {name}: {{ cidr: 10.{octet}.0.0/24{internet} }}").ok();
            self.networks.push(name.clone());
            self.used.insert(name, BTreeSet::new());
        }
    }

    /// A second octet no network has, recorded in `octets`.
    fn free_octet(&mut self, octets: &mut Vec<u32>) -> u32 {
        loop {
            let octet = self.rng.range(1, 250);
            if !octets.contains(&octet) {
                octets.push(octet);
                return octet;
            }
        }
    }

    /// The top-level `inputs` and `secrets`, declared before any machine references them.
    fn declarations(&mut self, out: &mut String) {
        if self.rng.chance(55) {
            out.push_str("inputs:\n  FLAG: {}\n  API_URL: { sensitive: false }\n");
            self.inputs.push("FLAG".to_owned());
            self.inputs.push("API_URL".to_owned());
        }
        if self.rng.chance(40) {
            out.push_str("secrets:\n  db: { env: DB_SOURCE }\n");
            self.secrets.push("db".to_owned());
        }
    }

    /// The `machines` block: one to four, each on a subset of the networks.
    fn machines_block(&mut self, out: &mut String) {
        let count = self.rng.range(1, 4);
        out.push_str("machines:\n");
        for i in 0..count {
            self.machines.push(format!("m{i}"));
        }
        let names = self.machines.clone();
        for (i, name) in names.iter().enumerate() {
            self.machine(out, name, i);
        }
    }

    /// One machine: its OS, networks, an optional app, ports, files, resources, dependency and
    /// setup. Machine `m0` is always Linux and places every declared input and secret, so each
    /// declaration is used (the format requires it).
    fn machine(&mut self, out: &mut String, name: &str, index: usize) {
        let windows = index != 0 && self.rng.chance(15);
        let os = if windows { self.rng.pick(&WINDOWS_OS) } else { self.rng.pick(&LINUX_OS) };
        writeln!(out, "  {name}:\n    os: {os}").ok();
        self.attach(out);
        self.app(out, name, windows);
        self.ports(out, index);
        self.files(out, index);
        self.environment(out, index);
        if self.rng.chance(40) {
            out.push_str("    resources: { cpus: 1, memory_mb: 1024 }\n");
        }
        self.depends_on(out, index);
        if self.rng.chance(45) {
            let ext = if windows { "ps1" } else { "sh" };
            writeln!(out, "    setup: [setup/{name}.{ext}]").ok();
        }
    }

    /// `networks: { n0: 10, ... }`: at least one network, each with a host octet unique in it.
    fn attach(&mut self, out: &mut String) {
        let nets = self.networks.clone();
        let want = self.rng.below(nets.len()).saturating_add(1);
        let mut chosen: Vec<String> = Vec::new();
        for net in &nets {
            if chosen.len() < want && (chosen.is_empty() || self.rng.chance(60)) {
                chosen.push(net.clone());
            }
        }
        if let (true, Some(first)) = (chosen.is_empty(), nets.first()) {
            chosen.push(first.clone());
        }
        let pairs: Vec<String> = chosen.iter().map(|net| format!("{net}: {}", self.free_host(net))).collect();
        writeln!(out, "    networks: {{ {} }}", pairs.join(", ")).ok();
    }

    /// A host octet (2..=250) no machine already uses on `net`.
    fn free_host(&mut self, net: &str) -> u32 {
        loop {
            let host = self.rng.range(2, 250);
            let used = self.used.entry(net.to_owned()).or_default();
            if used.insert(host) {
                return host;
            }
        }
    }

    /// The machine's `app`. Windows (no container form) gets a PowerShell `run` step or the plain
    /// OS; Linux gets an image, a build context, the configuration model, or nothing.
    fn app(&mut self, out: &mut String, name: &str, windows: bool) {
        if windows {
            if self.rng.chance(70) {
                writeln!(out, "    app: {{ steps: [{{ run: provision/{name}.ps1 }}] }}").ok();
            }
            return;
        }
        match self.rng.range(1, 4) {
            1 => {
                writeln!(out, "    app: {{ image: \"nginx:1.27-alpine\", steps: [{{ run: provision/{name}.sh }}] }}").ok();
            }
            2 => self.config_app(out),
            3 => {
                writeln!(out, "    app: {{ build: build/{name} }}").ok();
            }
            _ => {}
        }
    }

    /// An `app` written in the configuration model: a handful of guarded steps, one of them a
    /// `user` with a sentinel password when the leak check wants one.
    fn config_app(&mut self, out: &mut String) {
        let mut steps: Vec<String> = vec!["{ packages: [curl, ca-certificates] }".to_owned()];
        if self.rng.chance(60) {
            steps.push("{ file: { path: /srv/data, state: directory, mode: \"0755\" } }".to_owned());
        }
        if self.rng.chance(50) {
            steps.push("{ command: [sh, -c, \"echo configured\"], creates: /srv/data/stamp }".to_owned());
        }
        if self.rng.chance(50) {
            let value = self.plant_sentinel();
            steps.push(format!("{{ user: {{ name: editor, uid: 1600, group: staff, password: \"{value}\" }} }}"));
            steps.insert(0, "{ group: { name: staff, gid: 1600 } }".to_owned());
        }
        if self.rng.chance(40) {
            steps.push("{ service: { name: worker, run: [sleep, infinity] } }".to_owned());
        }
        writeln!(out, "    app:\n      steps: [{}]", steps.join(", ")).ok();
    }

    /// A fresh sentinel secret value, recorded so the leak check knows to look for it.
    fn plant_sentinel(&mut self) -> String {
        self.sentinel += 1;
        let value = format!("{SENTINEL_PREFIX}{:08x}", self.sentinel.wrapping_mul(0x9E37_79B9));
        self.secret_values.push(value.clone());
        value
    }

    /// An optional `ports` map: bare numbers, or a port with a protocol and perhaps a published
    /// host port drawn from the per-spec counter so none collide.
    fn ports(&mut self, out: &mut String, index: usize) {
        if !self.rng.chance(55) {
            return;
        }
        let count = self.rng.range(1, 3);
        let protocols = ["http", "https", "tcp", "udp"];
        let mut entries: Vec<String> = Vec::new();
        let mut waitable = false;
        for i in 0..count {
            let port = self.rng.range(1024, 60000);
            if self.rng.chance(40) {
                // A bare number is a TCP port, something a dependant can wait for.
                entries.push(format!("p{i}: {port}"));
                waitable = true;
            } else {
                let protocol = self.rng.pick(&protocols);
                waitable |= protocol != "udp";
                let publish = if self.rng.chance(40) {
                    self.host_port += 1;
                    format!(", publish: {}", self.host_port)
                } else {
                    String::new()
                };
                entries.push(format!("p{i}: {{ port: {port}, protocol: {protocol}{publish} }}"));
            }
        }
        if waitable {
            self.ported.insert(index);
        }
        writeln!(out, "    ports: {{ {} }}", entries.join(", ")).ok();
    }

    /// `files` reading declared inputs and secrets. Machine `m0` places `FLAG` and every secret,
    /// so each declaration is used; later machines may place them again.
    fn files(&mut self, out: &mut String, index: usize) {
        let first = index == 0;
        let mut files: Vec<String> = Vec::new();
        if self.inputs.iter().any(|i| i == "FLAG") && (first || self.rng.chance(50)) {
            files.push("{ path: /run/lab/flag, input: FLAG }".to_owned());
        }
        if !self.secrets.is_empty() && (first || self.rng.chance(50)) {
            let names = self.secrets.clone();
            for secret in names {
                files.push(format!("{{ path: /run/lab/{secret}, secret: {secret} }}"));
            }
        }
        if !files.is_empty() {
            writeln!(out, "    files: [{}]", files.join(", ")).ok();
        }
    }

    /// `environment` mapping the non-sensitive `API_URL` input in, on `m0`, so a declared input
    /// that isn't a file is still used.
    fn environment(&mut self, out: &mut String, index: usize) {
        if index == 0 && self.inputs.iter().any(|i| i == "API_URL") {
            out.push_str("    environment: { API_URL: { input: API_URL } }\n");
        }
    }

    /// An optional `depends_on` an earlier machine that has ports (acyclic by construction, and
    /// there must be something to wait for).
    fn depends_on(&mut self, out: &mut String, index: usize) {
        let earlier_with_ports: Vec<String> = self
            .ported
            .iter()
            .copied()
            .filter(|&i| i < index)
            .filter_map(|i| self.machines.get(i).cloned())
            .collect();
        if !earlier_with_ports.is_empty() && self.rng.chance(45) {
            let at = self.rng.below(earlier_with_ports.len());
            if let Some(on) = earlier_with_ports.get(at) {
                writeln!(out, "    depends_on: [{on}]").ok();
            }
        }
    }

    /// An optional `checks` block: a project script, and now and then a declared check from a
    /// machine.
    fn checks(&mut self, out: &mut String) {
        if !self.rng.chance(50) {
            return;
        }
        out.push_str("checks:\n  - checks/all.sh\n");
        if let Some(from) = self.machines.first().filter(|_| self.rng.chance(50)) {
            writeln!(out, "  - {{ name: it answers, from: {from}, exec: \"echo ok\", expect: ok }}").ok();
        }
    }

    /// An optional per-target `overrides` block: a Docker network moved to another `/24`.
    fn overrides(&mut self, out: &mut String) {
        if !self.rng.chance(30) {
            return;
        }
        let Some(net) = self.networks.first() else { return };
        // 251..=254 is a range `free_octet` (1..=250) never gives a real network, so this moved
        // Docker network can't overlap another.
        let octet = self.rng.range(251, 254);
        writeln!(out, "overrides:\n  docker:\n    networks:\n      {net}: {{ cidr: 10.{octet}.0.0/24 }}").ok();
    }
}
