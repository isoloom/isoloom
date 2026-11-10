//! The servers' run manifests side by side: the seal is one and the same on Proxmox and ESXi.

use isoloom_manifest::{Action, Condition, Manifest, Run, SetBy};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::{EsxiBackend, ProxmoxBackend};

/// A lab with nothing to seal.
const OPEN: &str = "version: 1
name: shop
networks:
  lan: { cidr: 10.60.0.0/24 }
machines:
  web: { networks: { lan: 10 }, os: ubuntu@24.04, app: { steps: [{ run: web.sh }] } }
checks:
  - { http: http://web/, expect: 200, wait: 120 }
";

/// A lab with an `internet: false` network, so the run is sealed: `cache` is on `back` alone.
const SEALED: &str = "version: 1
name: seg
networks:
  front: { cidr: 10.61.10.0/24 }
  back: { cidr: 10.61.20.0/24, internet: false }
machines:
  web: { networks: { front: 10 }, os: ubuntu@24.04, app: { steps: [{ run: web.sh }] } }
  cache: { networks: { back: 20 }, os: ubuntu@24.04, app: { steps: [{ run: cache.sh }] } }
";

/// The servers.
const SERVERS: [Target; 2] = [Target::Proxmox, Target::Esxi];

/// The run manifest of `yaml` on server `target`.
fn manifest(yaml: &str, target: Target) -> Manifest {
    let env = crate::testing::environment(yaml, target);
    let m = match target {
        Target::Esxi => EsxiBackend.run_manifest(&env),
        _ => ProxmoxBackend.run_manifest(&env),
    };
    let m = m.unwrap().expect("a server writes a run manifest");
    m.validate().unwrap();
    m
}

#[test]
fn an_unsealed_run_has_no_seal_step_and_no_sealed_param() {
    for target in SERVERS {
        let m = manifest(OPEN, target);
        assert!(m.lifecycle.seal.is_none(), "{target:?}: nothing to seal");
        assert!(m.params.iter().all(|p| p.name.as_str() != "isoloom_sealed"), "{target:?}");
    }
}

#[test]
fn the_seal_applies_the_sealed_ruleset_live_on_the_router() {
    for target in SERVERS {
        let m = manifest(SEALED, target);
        // The runner sets the seal param once the machines are ready and records it.
        let seal = m.lifecycle.seal.as_ref().expect("a run with an internet:false network seals");
        assert_eq!(seal.param.as_str(), "isoloom_sealed", "{target:?}");
        assert!(
            m.params.iter().any(|p| p.name.as_str() == "isoloom_sealed" && p.set_by == SetBy::Runner),
            "{target:?}"
        );
        // One step, no second apply (the module never changes for the seal): an ssh.run that
        // applies the sealed ruleset live to the already-booted router and keeps it for later boots.
        let ids: Vec<&str> = seal.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["seal-reload"], "{target:?}");
        let reload = &seal.steps.as_slice()[0];
        let Run::Action { action: Action::SshRun, with } = &reload.run else {
            panic!("{target:?}: {:?}", reload.run)
        };
        let arg = |k: &str| with.iter().find(|(n, _)| n.as_str() == k).map(|(_, v)| v.to_string());
        assert_eq!(
            arg("host").as_deref(),
            Some("${outputs.ip}"),
            "{target:?}: reached at the router's uplink, the environment's way in"
        );
        assert_eq!(arg("user").as_deref(), Some("isoloom"), "{target:?}");
        assert!(arg("via").is_none(), "{target:?}: the router is reached directly, not through itself");
        let command = arg("command").unwrap();
        assert!(command.contains("nft -f /etc/nftables.sealed.nft"), "{target:?}: {command}");
        assert!(
            command.contains("cp /etc/nftables.sealed.nft /etc/nftables.conf"),
            "{target:?}: kept for later boots: {command}"
        );
        // The way in it names is an output of the run, the router's uplink address.
        assert!(m.outputs.iter().any(|o| o.name.as_str() == "ip"), "{target:?}");
    }
}

/// ESXi seals as Proxmox does: the same seal, step for step, and the same param.
#[test]
fn esxi_seals_as_proxmox_does() {
    let (pve, esxi) = (manifest(SEALED, Target::Proxmox), manifest(SEALED, Target::Esxi));
    assert_eq!(esxi.lifecycle.seal, pve.lifecycle.seal);
    let sealed = |m: &Manifest| m.params.iter().find(|p| p.name.as_str() == "isoloom_sealed").cloned();
    assert_eq!(sealed(&esxi), sealed(&pve));
    // The same ready files, but for the log each server's set-up writes.
    let unlogged = |m: &Manifest| -> Vec<Condition> {
        let mut ready = m.ready[1..].to_vec();
        for c in &mut ready {
            if let Condition::ReadyFile { log, .. } = c {
                *log = None;
            }
        }
        ready
    };
    assert_eq!(unlogged(&esxi), unlogged(&pve), "the seal comes once the same set-ups are done");
}

/// Each ready file names the log the server's set-up writes: cloud-init's on Proxmox, where
/// cloud-init runs it; none on ESXi, where Terraform runs it over SSH and its output is the
/// apply's own (cloud-init there runs only the router's set-up, so its log says nothing of a
/// machine's step).
#[test]
fn a_ready_file_names_the_log_its_servers_set_up_writes() {
    let logs = |target| -> Vec<Option<String>> {
        manifest(OPEN, target)
            .ready
            .iter()
            .filter_map(|c| match c {
                Condition::ReadyFile { log, .. } => Some(log.as_ref().map(ToString::to_string)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(logs(Target::Proxmox), [Some("/var/log/cloud-init-output.log".to_owned())]);
    assert_eq!(logs(Target::Esxi), [None]);
}
