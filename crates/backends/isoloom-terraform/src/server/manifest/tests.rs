//! The servers' run manifests and modules side by side: the seal is one and the same on Proxmox
//! and ESXi, the module's own.

use isoloom_manifest::{Condition, Manifest};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;

use crate::{EsxiBackend, ProxmoxBackend};

/// An environment with nothing to seal.
const OPEN: &str = "version: 1
name: shop
networks:
  lan: { cidr: 10.60.0.0/24 }
machines:
  web: { networks: { lan: 10 }, os: ubuntu@24.04, app: { steps: [{ run: web.sh }] } }
checks:
  - { http: http://web/, expect: 200, wait: 120 }
";

/// An environment with an `internet: false` network, so the run is sealed: `cache` is on `back` alone.
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

/// The module of `yaml` on server `target`.
fn module(yaml: &str, target: Target) -> String {
    let env = crate::testing::environment(yaml, target);
    let files = match target {
        Target::Esxi => EsxiBackend.render(&env),
        _ => ProxmoxBackend.render(&env),
    };
    files.unwrap().into_iter().find(|f| f.path.ends_with("main.tf")).expect("a module").contents
}

#[test]
fn an_unsealed_run_has_no_seal_step_and_no_seal() {
    for target in SERVERS {
        let m = manifest(OPEN, target);
        assert!(m.lifecycle.seal.is_none(), "{target:?}: nothing to seal");
        assert!(!module(OPEN, target).contains("isoloom_seal"), "{target:?}");
    }
}

/// The seal is the module's own, within the one apply: once the machines are set up (Proxmox:
/// each readiness wait; ESXi: each machine's set-up over SSH), `terraform_data.isoloom_seal`
/// signs in to the router at its uplink address and applies the sealed ruleset live, keeping it
/// for later boots. The run manifest has no seal of its own.
#[test]
fn the_module_seals_on_the_router_once_the_machines_are_set_up() {
    for (target, after) in [
        (
            Target::Proxmox,
            "depends_on = [terraform_data.isoloom_ready_web, terraform_data.isoloom_ready_cache]",
        ),
        (Target::Esxi, "depends_on = [terraform_data.web, terraform_data.cache]"),
    ] {
        assert!(manifest(SEALED, target).lifecycle.seal.is_none(), "{target:?}");
        let tf = module(SEALED, target);
        let (_, seal) = tf.split_once("resource \"terraform_data\" \"isoloom_seal\"").expect("a seal");
        let (seal, _) = seal.split_once("\n}\n").unwrap();
        assert!(
            seal.contains("host        = local.router_address") && seal.contains("user        = \"isoloom\""),
            "{target:?}: {seal}"
        );
        assert!(
            seal.contains("nft -f /etc/nftables.sealed.nft && cp /etc/nftables.sealed.nft /etc/nftables.conf"),
            "{target:?}: {seal}"
        );
        assert!(seal.contains(after), "{target:?}: {seal}");
    }
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
