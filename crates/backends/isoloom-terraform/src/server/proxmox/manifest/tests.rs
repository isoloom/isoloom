use isoloom_manifest::{Action, Condition, Delivery, OutputSource, Run, SetBy, SlotKind, TfVarShape};
use isoloom_plan::{Environment, Lab};
use isoloom_spec::model::Target;

use super::*;

fn env(yaml: &str) -> Environment {
    let lab = Lab::new(isoloom_spec::parse(yaml).unwrap());
    assert!(lab.problems().is_empty(), "{:?}", lab.problems());
    Environment::build(&lab.for_target(Target::Proxmox), Target::Proxmox, None)
}

const LAB: &str = "version: 1
name: shop
inputs: { FLAG: { sensitive: false } }
networks:
  lan: { cidr: 10.60.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    os: debian@12
    environment: { FLAG: { input: FLAG } }
    ports: { site: { port: 80, protocol: http, publish: 8080 } }
    app: { steps: [{ run: web.sh }] }
checks:
  - { http: http://web/, expect: 200, wait: 120 }
";

#[test]
fn the_module_is_applied_with_the_runners_state_and_values() {
    let m = manifest(&env(LAB)).unwrap();
    m.validate().unwrap();
    assert!(matches!(&m.identity, Identity::Terraform(t) if t.module.as_str() == "." && t.state == StateOwner::Runner));
    let ids: Vec<&str> = m.lifecycle.up.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["clear-lock", "init", "apply"]);
    let apply = &m.lifecycle.up.as_slice()[2];
    let env_of = |k: &str| apply.env.iter().find(|(n, _)| n.as_str() == k).map(|(_, v)| v.to_string());
    assert_eq!(env_of("TF_VAR_slot").as_deref(), Some("${run.slot}"));
    assert_eq!(env_of("TF_VAR_ssh_public_key").as_deref(), Some("${run.ssh_public_key}"));
    assert_eq!(env_of("TF_DATA_DIR").as_deref(), Some("${run.state_dir}/.terraform"));
    let init = isoloom_manifest::to_json(&m).unwrap();
    assert!(init.contains("-backend-config=path=${run.state_dir}/terraform.tfstate"), "{init}");
    assert!(matches!(
        m.lifecycle.status.run,
        Run::Action {
            action: Action::TerraformReadState,
            ..
        }
    ));
    assert!(m.lifecycle.park.is_none(), "a server run is created or destroyed");
    let destroy = isoloom_manifest::to_json(&m).unwrap();
    assert!(destroy.contains("\"-refresh=false\""), "a destroy never waits on a router's missing agent");
}

#[test]
fn terraform_runs_from_its_pinned_image_on_macos() {
    let m = manifest(&env(LAB)).unwrap();
    let tf = m.tools.iter().find(|t| t.name.as_str() == "terraform").unwrap();
    assert!(tf.container.is_some() && tf.container_hosts.contains(&HostOs::Macos));
    assert_eq!(tf.min_version.as_ref().map(Version::as_str), Some("1.6.0"));
}

#[test]
fn the_variables_are_params_and_the_secrets_slots() {
    let m = manifest(&env(LAB)).unwrap();
    let param = |n: &str| m.params.iter().find(|p| p.name.as_str() == n);
    let node = param("node").unwrap();
    assert!(node.required && node.default.is_none(), "no default node");
    assert_eq!(param("uplink_bridge").unwrap().default.as_deref(), Some("vmbr0"));
    assert_eq!(param("proxmox_insecure").unwrap().values, ["true", "false"]);
    for runner_set in ["slot", "ssh_public_key", "proxmox_ssh_private_key_file", "proxmox_password", "inputs"] {
        assert!(param(runner_set).is_none(), "{runner_set}");
    }
    assert!(m.params.iter().all(|p| p.set_by == SetBy::Embedder));
    let slot = |n: &str| m.secrets.iter().find(|s| s.name.as_str() == n).unwrap();
    assert_eq!(slot("proxmox_api_token").kind, SlotKind::Connection);
    assert!(slot("proxmox_password").sensitive);
    let flag = slot("FLAG");
    assert_eq!(flag.kind, SlotKind::Input);
    assert!(!flag.sensitive, "the spec's word on it");
    assert!(matches!(flag.deliver.as_slice(), [Delivery::TfVar { var, shape: TfVarShape::MapEntry }] if var.as_str() == "inputs"));
}

#[test]
fn ready_means_the_apply_then_each_machines_ready_file_through_the_router() {
    let m = manifest(&env(LAB)).unwrap();
    assert!(matches!(&m.ready[0], Condition::StepOk { step, .. } if step.as_str() == "apply"));
    let Condition::ReadyFile {
        host,
        via,
        path,
        unreachable_timeout_s,
        ..
    } = &m.ready[1]
    else {
        panic!("{:?}", m.ready)
    };
    assert_eq!(host.to_string(), "10.60.0.10");
    assert_eq!(via.as_ref().map(ToString::to_string).as_deref(), Some("isoloom@${outputs.ip}"));
    assert_eq!(path.to_string(), "${outputs.ready_file}");
    assert!(unreachable_timeout_s.is_some(), "an unreachable host fails, it isn't ready");
}

#[test]
fn outputs_come_from_the_state_and_the_published_ports_from_the_router() {
    let m = manifest(&env(LAB)).unwrap();
    let ip = m.outputs.iter().find(|o| o.name.as_str() == "ip").unwrap();
    assert!(matches!(&ip.from, OutputSource::TerraformOutput { name, .. } if name.as_str() == "ip"));
    let site = m.outputs.iter().find(|o| o.name.as_str() == "web_site").unwrap();
    assert_eq!(site.format.as_ref().map(ToString::to_string).as_deref(), Some("http://${outputs.ip}:${value}/"));
    let web = m.machines.iter().find(|x| x.name.as_str() == "web").unwrap();
    assert_eq!(
        web.ports[0].publish.as_ref().and_then(|p| p.preferred).map(std::num::NonZeroU16::get),
        Some(8080)
    );
    assert!(m.machines.iter().any(|x| x.name.as_str() == "isoloom-router"));
}

#[test]
fn each_check_runner_is_piped_to_its_machine_through_the_router() {
    let m = manifest(&env(LAB)).unwrap();
    let r = &m.checks.runners[0];
    let Run::Action { action: Action::SshRun, with } = &r.step.run else {
        panic!("{:?}", r.step)
    };
    let arg = |k: &str| with.iter().find(|(n, _)| n.as_str() == k).map(|(_, v)| v.to_string());
    assert_eq!(arg("via").as_deref(), Some("isoloom@${outputs.ip}"));
    assert_eq!(arg("command").as_deref(), Some("ISOLOOM_DERIVED=${params.ISOLOOM_DERIVED} sh -s"));
    assert_eq!(r.step.stdin.as_ref().map(RelPath::as_str), Some("checks/networks.sh"));
    let waits: u32 = r.checks.iter().map(|c| c.wait_s).sum();
    assert!(r.step.timeout_s.unwrap().get() >= waits + 60);
}
