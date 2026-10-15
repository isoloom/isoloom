//! The tiers: how far each target has been proven, per provider or cloud where it has them.
//!
//! One list, `tiers.json` beside this crate's manifest, embedded here. Each entry names the host
//! operating systems the target exists on, the tier 1.0 aims for (its `goal`), and the real runs
//! that prove it. The tier itself is never written down: it is worked out from the runs, so it
//! can't claim more than they show.
//!
//! - **Tier 1:** the latest run on every host OS the entry exists on passed.
//! - **Tier 2:** generated, and checked by the official validators (`cargo xtask verify`).
//! - **Tier 3:** experimental.
//!
//! `cargo xtask tiers record <entry> <host> <result>` adds a run to the list.

mod file;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fmt;

use isoloom_manifest::HostOs;
use isoloom_spec::model::Target;
use serde_json::{Value, json};

use crate::Result;

pub use file::{Date, RunResult};

/// The list, as `cargo xtask tiers record` writes it.
const DATA: &str = include_str!("../tiers.json");

/// How far a target has been proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Proven by real runs on every host OS it exists on.
    One,
    /// Generated, and checked by the official validators.
    Two,
    /// Experimental.
    Three,
}

impl Tier {
    /// The tier's number.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Tier::One => 1,
            Tier::Two => 2,
            Tier::Three => 3,
        }
    }

    /// What the tier means, in a few words.
    #[must_use]
    pub fn meaning(self) -> &'static str {
        match self {
            Tier::One => "proven by real runs on every host OS it exists on",
            Tier::Two => "generated and validated",
            Tier::Three => "experimental",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.number())
    }
}

/// A real run of a target's generated files, as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Run {
    /// The machine that ran the tools (`mac`, `jarvis`, `karen`).
    pub host: String,
    /// Its operating system.
    pub os: HostOs,
    /// The day it ran.
    pub date: Date,
    /// Whether it passed.
    pub result: RunResult,
    /// The server it ran on, for a server target (`pve-lab`).
    pub server: Option<String>,
    /// What ran, in a few words.
    pub note: Option<String>,
}

/// A target, or one of its providers or clouds, with its tier's evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TierEntry {
    /// The target.
    pub target: Target,
    /// Its provider or cloud (`libvirt`, `aws`), for a target that has them.
    pub variant: Option<&'static str>,
    /// The tier 1.0 aims for: 1 for the launcher's targets.
    pub goal: Tier,
    /// The host operating systems it exists on.
    pub exists_on: BTreeSet<HostOs>,
    /// Whether it is experimental (tier 3).
    pub experimental: bool,
    /// Its real runs, oldest first.
    pub runs: Vec<Run>,
}

/// The name of a target, or of one of its providers or clouds: the target's id, then `/` and
/// the provider or cloud (`vagrant/libvirt`). What tier entries and matrix columns are called.
pub(crate) fn name(target: Target, variant: Option<&str>) -> String {
    match variant {
        Some(v) => format!("{}/{v}", target.id()),
        None => target.id().to_owned(),
    }
}

impl TierEntry {
    /// Its name: the target's id, then `/` and its provider or cloud (`vagrant/libvirt`).
    #[must_use]
    pub fn id(&self) -> String {
        name(self.target, self.variant)
    }

    /// The latest run on each host OS, by OS.
    fn latest(&self) -> Vec<&Run> {
        let mut latest: Vec<&Run> = Vec::new();
        for run in &self.runs {
            match latest.iter_mut().find(|r| r.os == run.os) {
                Some(seen) if seen.date <= run.date => *seen = run,
                Some(_) => {}
                None => latest.push(run),
            }
        }
        latest.sort_by_key(|r| r.os);
        latest
    }

    /// The host OSes whose latest run passed.
    #[must_use]
    pub fn proven(&self) -> BTreeSet<HostOs> {
        self.latest().into_iter().filter(|r| r.result == RunResult::Pass).map(|r| r.os).collect()
    }

    /// Its tier, from the runs: 1 once the latest run on every host OS it exists on passed.
    #[must_use]
    pub fn tier(&self) -> Tier {
        if self.experimental {
            Tier::Three
        } else if !self.exists_on.is_empty() && self.exists_on.is_subset(&self.proven()) {
            Tier::One
        } else {
            Tier::Two
        }
    }

    /// The host OSes still to prove, for an entry that aims for tier 1 and isn't there yet.
    #[must_use]
    pub fn pending(&self) -> BTreeSet<HostOs> {
        if self.goal != Tier::One || self.tier() != Tier::Two {
            return BTreeSet::new();
        }
        self.exists_on.difference(&self.proven()).copied().collect()
    }

    /// What proves it, in a line: the latest run on each host OS (`real runs: linux (friday,
    /// 2026-10-15)`), or `validated only`; then the host OSes tier 1 waits for.
    #[must_use]
    pub fn evidence(&self) -> String {
        let runs: Vec<String> = self.latest().into_iter().map(shown).collect();
        let mut parts = vec![if self.experimental {
            "experimental".to_owned()
        } else if runs.is_empty() {
            "validated only".to_owned()
        } else {
            format!("real runs: {}", runs.join(", "))
        }];
        let pending: Vec<&str> = self.pending().into_iter().map(os_id).collect();
        if !pending.is_empty() {
            parts.push(format!("tier 1 pending: {}", pending.join(", ")));
        }
        parts.join("; ")
    }
}

impl TierEntry {
    /// The entry as JSON (`isoloom targets --json`, `isoloom matrix --json`): its name, target and
    /// provider or cloud, tier and goal, the host OSes it exists on and those proven or pending,
    /// the evidence in a line, and the runs.
    #[must_use]
    pub fn json(&self) -> Value {
        let oses = |set: &BTreeSet<HostOs>| set.iter().map(|o| os_id(*o)).collect::<Vec<_>>();
        let runs: Vec<Value> = self
            .runs
            .iter()
            .map(|r| {
                json!({
                    "host": r.host,
                    "os": os_id(r.os),
                    "date": r.date.as_str(),
                    "result": if r.result == RunResult::Pass { "pass" } else { "fail" },
                    "server": r.server,
                    "note": r.note,
                })
            })
            .collect();
        json!({
            "entry": self.id(),
            "target": self.target.id(),
            "variant": self.variant,
            "tier": self.tier().number(),
            "goal": self.goal.number(),
            "exists_on": oses(&self.exists_on),
            "proven": oses(&self.proven()),
            "pending": oses(&self.pending()),
            "evidence": self.evidence(),
            "runs": runs,
        })
    }
}

/// A run in a line: `macos (mac, pve-lab, 2026-10-15)`, `failed` when it did.
fn shown(run: &Run) -> String {
    let server = run.server.as_deref().map(|s| format!(", {s}")).unwrap_or_default();
    let failed = if run.result == RunResult::Fail { "failed " } else { "" };
    format!("{failed}{} ({}{server}, {})", os_id(run.os), run.host, run.date)
}

/// A host OS by the name `tiers.json` and the run manifest write it with.
#[must_use]
pub fn os_id(os: HostOs) -> &'static str {
    match os {
        HostOs::Linux => "linux",
        HostOs::Macos => "macos",
        HostOs::Windows => "windows",
    }
}

/// Every entry, in the order targets are listed, each target's providers or clouds in turn.
///
/// # Errors
///
/// [`crate::Error::Tiers`] when `tiers.json` isn't the list this crate reads (a test keeps it
/// readable).
pub fn tiers() -> Result<Vec<TierEntry>> {
    file::parse(DATA)
}

/// The entries of `target`, from `entries`.
#[must_use]
pub fn of(entries: &[TierEntry], target: Target) -> Vec<&TierEntry> {
    entries.iter().filter(|e| e.target == target).collect()
}

/// The Vagrant providers the Vagrantfile has a block for, as `vagrant up --provider` names them,
/// in the order the launcher offers them.
pub(crate) const VAGRANT_PROVIDERS: [&str; 8] = ["virtualbox", "vmware_desktop", "hyperv", "parallels", "libvirt", "qemu", "utm", "vmware_esxi"];

/// Where `docker-vm`'s VM runs: the Vagrant providers its Vagrantfile has a block for (no
/// Hyper-V), then the Proxmox module.
const DOCKER_VM: [&str; 8] = ["virtualbox", "vmware_desktop", "parallels", "utm", "qemu", "libvirt", "vmware_esxi", "proxmox"];

/// The clouds `cloud-services` reads (`cloud.provider`).
const CLOUD_SERVICES: [&str; 3] = ["aws", "azure", "gcp"];

/// The providers or clouds `target` has an entry for each of; empty for a target that has none.
#[must_use]
pub fn variants(target: Target) -> Vec<&'static str> {
    match target {
        Target::Vagrant => VAGRANT_PROVIDERS.to_vec(),
        Target::DockerVm => DOCKER_VM.to_vec(),
        Target::CloudVm | Target::CloudDocker => isoloom_terraform::Cloud::ALL.iter().map(|c| c.id()).collect(),
        Target::CloudServices => CLOUD_SERVICES.to_vec(),
        Target::Docker | Target::Hosted | Target::Kubernetes | Target::Hybrid | Target::Proxmox | Target::Esxi | Target::External => Vec::new(),
    }
}
