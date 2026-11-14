//! The drawing's nodes as Isoloom machines.
//!
//! A container node is a custom app from its image (`app.image`), kept running idle with its own
//! `command` when the image runs nothing of its own; a Cisco node is an appliance (`app.appliance`
//! with its image, or the firmware Dynamips boots). A VM node is its OS (`os`), set up by its
//! provisioning steps (`app.steps`): each target decides how to deliver it. A Windows machine's
//! playbooks run from Isoloom's controller instead (see `controller`).

use isoloom_spec::Appliance;
use isoloom_spec::syntax::{OsRef, ProjectPath, is_yaml_file};

use crate::controller::ControllerPlaybook;
use crate::document::{AppDoc, MachineDoc, StepDoc};
use crate::draft::Draft;
use crate::project::{config_path, firmware_path, own_config};
use crate::{Error, Kind, Node, Result, Topology, ports};

/// What keeps an idle container running: it has no service of its own.
const IDLE_COMMAND: [&str; 2] = ["sleep", "infinity"];

/// Every node: a machine (and the playbooks the controller runs for it), or an automatic router
/// (which only records its networks).
pub(crate) fn add(topo: &Topology, draft: &mut Draft) -> Result<()> {
    for (name, node) in &topo.nodes {
        if let Kind::Router {} = node.kind {
            draft.routers.insert(name.clone(), node.networks.keys().cloned().collect());
        } else {
            let (machine, playbooks) = machine(name, node)?;
            draft.machines.insert(name.clone(), machine);
            draft.controller.extend(playbooks);
        }
    }
    Ok(())
}

/// A machine node as a machine, without its networks yet, and the playbooks the controller runs
/// for it.
fn machine(name: &str, node: &Node) -> Result<(MachineDoc, Vec<ControllerPlaybook>)> {
    let mut m = MachineDoc {
        access: node.access,
        ports: ports::named(name, &node.services)?,
        ..MachineDoc::default()
    };
    let playbooks = match &node.kind {
        Kind::Container {
            image,
            appliance: Some(kind),
            config,
            ..
        } => {
            m.app = Some(appliance(name, image, *kind, config.as_deref()));
            Vec::new()
        }
        Kind::Container { image, idle, .. } => {
            m.app = Some(AppDoc {
                image: Some(image.clone()),
                ..AppDoc::default()
            });
            m.command = idle.then(|| IDLE_COMMAND.map(str::to_owned).to_vec());
            Vec::new()
        }
        Kind::Vm { os, provision } => {
            let (steps, playbooks) = vm_steps(name, *os, provision)?;
            m.os = Some(*os);
            m.app = (!steps.is_empty()).then(|| AppDoc { steps, ..AppDoc::default() });
            playbooks
        }
        Kind::Router {} => Vec::new(),
    };
    Ok((m, playbooks))
}

/// A VM's provisioning steps, in order: its own, and on Windows the playbooks the controller runs.
/// A path that isn't one of the project as Isoloom reads it ([`ProjectPath`]) is refused, whatever
/// the OS.
fn vm_steps(name: &str, os: OsRef, provision: &[String]) -> Result<(Vec<StepDoc>, Vec<ControllerPlaybook>)> {
    let mut steps = Vec::new();
    let mut playbooks = Vec::new();
    for path in provision {
        path.parse::<ProjectPath>().map_err(|source| Error::StepPath {
            node: name.to_owned(),
            path: path.clone(),
            source: Box::new(source),
        })?;
        match step(path) {
            StepDoc::Playbook(path) if os.is_windows() => playbooks.push(ControllerPlaybook {
                path,
                machine: name.to_owned(),
            }),
            own => steps.push(own),
        }
    }
    Ok((steps, playbooks))
}

/// A provisioning step: an Ansible playbook (`.yml`, `.yaml`, Isoloom's [`is_yaml_file`]), else a
/// script.
fn step(path: &str) -> StepDoc {
    if is_yaml_file(path) {
        StepDoc::Playbook(path.to_owned())
    } else {
        StepDoc::Run(path.to_owned())
    }
}

/// A Cisco node: its image (Dynamips has none: Isoloom builds the emulator, and the user's IOS
/// image goes in the project's `images/` folder) and its own configuration's file.
fn appliance(name: &str, image: &str, kind: Appliance, config: Option<&str>) -> AppDoc {
    let dynamips = kind == Appliance::CiscoDynamips;
    AppDoc {
        image: (!dynamips).then(|| image.to_owned()),
        appliance: Some(kind),
        config: own_config(config).map(|_| config_path(name)),
        firmware: dynamips.then(|| firmware_path(name)),
        steps: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_yaml_step_is_a_playbook_whatever_its_case_and_any_other_a_script() {
        assert_eq!(step("provision/site.YML"), StepDoc::Playbook("provision/site.YML".into()));
        assert_eq!(step("provision/site.yaml"), StepDoc::Playbook("provision/site.yaml".into()));
        assert_eq!(step("provision/web.sh"), StepDoc::Run("provision/web.sh".into()));
        assert_eq!(step("provision/yml"), StepDoc::Run("provision/yml".into()));
    }

    #[test]
    fn a_windows_vms_playbooks_run_from_the_controller_and_its_scripts_on_it() {
        let provision = ["provision/a.ps1".to_owned(), "provision/b.yml".to_owned(), "provision/c.ps1".to_owned()];
        let windows: OsRef = "windows-server@2022".parse().unwrap();
        let (own, playbooks) = vm_steps("dc01", windows, &provision).unwrap();
        assert_eq!(own, [StepDoc::Run("provision/a.ps1".into()), StepDoc::Run("provision/c.ps1".into())]);
        assert_eq!(
            playbooks,
            [ControllerPlaybook {
                path: "provision/b.yml".into(),
                machine: "dc01".into()
            }]
        );
        let linux: OsRef = "debian@12".parse().unwrap();
        let (linux_own, linux_playbooks) = vm_steps("web", linux, &provision).unwrap();
        assert_eq!(linux_own.len(), 3);
        assert!(linux_playbooks.is_empty(), "a Linux machine runs its playbooks itself");
    }

    #[test]
    fn a_dynamips_router_boots_the_users_firmware_and_another_appliance_its_image() {
        let dynamips = appliance("r1", "ignored", Appliance::CiscoDynamips, None);
        assert_eq!((dynamips.image, dynamips.firmware), (None, Some("images/r1.bin".to_owned())));
        let iol = appliance("r2", "vrnetlab/cisco_iol:17.12.01", Appliance::CiscoIol, Some("  \n"));
        assert_eq!(
            (iol.image.as_deref(), iol.config, iol.firmware),
            (Some("vrnetlab/cisco_iol:17.12.01"), None, None)
        );
    }
}
