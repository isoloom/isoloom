//! True forms: one spec renders to true VMs on the VM targets and true containers on the Docker
//! targets (LXC containers on Proxmox where the spec asks), never a container inside a helper VM,
//! and a machine with no form for a target is refused there.

use std::collections::BTreeSet;

use isoloom_plan::MachineShape;
use isoloom_spec::model::{Shape, Target};
use isoloom_targets::{Lab, generate, refusal};

use crate::testing::{Outcome, between, example, examples, generated, lab};

/// The forms a machine of `target` may take: the target's own, nothing in between.
fn true_forms(target: Target) -> &'static [MachineShape] {
    match target.needs() {
        Shape::Docker => &[MachineShape::Container],
        Shape::Vm => &[MachineShape::Vm],
        Shape::VmOrLxc => &[MachineShape::Vm, MachineShape::Container],
        Shape::External => &[MachineShape::External],
    }
}

/// Every example's name.
fn example_names() -> Outcome<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(examples())?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|d| d.join("isoloom.yml").is_file())
        .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    Ok(names)
}

/// Each machine `lab` plans for `target`, by name, with its form.
fn forms(lab: &Lab, target: Target) -> Outcome<Vec<(String, MachineShape)>> {
    let env = lab.plan(&lab.for_target(target), target, None)?;
    Ok(env.machines.iter().map(|(name, mp)| (name.clone(), mp.shape)).collect())
}

#[test]
fn every_example_plans_each_machine_in_its_targets_true_form() -> Outcome {
    let mut reached = BTreeSet::new();
    let mut looked = 0;
    for name in example_names()? {
        let lab = example(&name)?;
        for target in lab.possible() {
            for (machine, shape) in forms(&lab, target)? {
                assert!(
                    true_forms(target).contains(&shape),
                    "{name} on {}: `{machine}` is {shape:?}, not one of {:?}",
                    target.id(),
                    true_forms(target)
                );
                reached.insert((target.id(), format!("{shape:?}")));
                looked += 1;
            }
        }
    }
    assert!(looked > 100, "the examples planned only {looked} machines");
    // Every target's true form is reached by some example (Proxmox's LXC form below).
    for target in Target::ALL {
        let shape = format!("{:?}", true_forms(target)[0]);
        assert!(reached.contains(&(target.id(), shape.clone())), "no example plans a {shape} on {}", target.id());
    }
    Ok(())
}

/// Machine `m` with `form` (YAML fields), alone on a network.
fn one(form: &str) -> Outcome<Lab> {
    lab(&format!(
        "version: 1\nname: forms\nnetworks: {{ lan: {{ cidr: 10.30.0.0/24 }} }}\nmachines:\n  m: {{ networks: {{ lan: 10 }}, {form} }}\n"
    ))
}

#[test]
fn a_machine_with_both_forms_is_a_container_on_docker_and_a_vm_on_the_vm_targets() -> Outcome {
    let both = one("os: debian@12, app: { image: nginx, steps: [{ run: s.sh }] }")?;
    for target in Target::ALL.into_iter().filter(|t| *t != Target::External) {
        let want = if target.needs() == Shape::Docker {
            MachineShape::Container
        } else {
            MachineShape::Vm
        };
        assert_eq!(forms(&both, target)?, [("m".to_owned(), want)], "{}", target.id());
    }
    Ok(())
}

#[test]
fn proxmox_runs_a_container_form_as_lxc_only_where_the_spec_asks() -> Outcome {
    let lxc = one("os: debian@12, app: { image: nginx, steps: [{ run: s.sh }] }")?;
    assert_eq!(forms(&lxc, Target::Proxmox)?, [("m".to_owned(), MachineShape::Vm)]);
    let asked = lab(
        "version: 1\nname: forms\nnetworks: { lan: { cidr: 10.30.0.0/24 } }\nmachines:\n  m: { networks: { lan: 10 }, app: { image: 'nginx:1.27' } }\noverrides:\n  proxmox: { machines: { m: { lxc: true } } }\n",
    )?;
    // An image alone has no VM form: only Proxmox's container form takes it, where asked.
    assert_eq!(refusal(&asked, Target::Proxmox)?, None);
    assert_eq!(forms(&asked, Target::Proxmox)?, [("m".to_owned(), MachineShape::Container)]);
    let tf = generated(&asked, Target::Proxmox, ".isoloom/proxmox/main.tf")?;
    let module = isoloom_test_support::hcl_body(&tf);
    assert_eq!(
        isoloom_test_support::hcl_names(&module, "resource", "proxmox_virtual_environment_container"),
        ["m"]
    );
    Ok(())
}

#[test]
fn a_container_only_machine_is_refused_on_the_vm_targets_never_put_in_a_helper_vm() -> Outcome {
    let image_only = one("app: { image: nginx }")?;
    assert!(generate(&image_only, Target::Docker).is_ok());
    for target in [Target::Vagrant, Target::Esxi, Target::CloudVm] {
        let why = refusal(&image_only, target)?.ok_or_else(|| format!("{} takes an image-only machine", target.id()))?;
        assert!(why.starts_with("machines.m.app.image: "), "{}: {why}", target.id());
    }
    Ok(())
}

#[test]
fn a_vm_only_machine_is_refused_on_the_docker_targets() -> Outcome {
    let windows = one("os: windows-server@2022")?;
    for target in [Target::Docker, Target::Hosted, Target::CloudDocker, Target::Kubernetes] {
        let why = refusal(&windows, target)?.ok_or_else(|| format!("{} takes a Windows VM", target.id()))?;
        assert_eq!(why, "machines.m.os: Windows has no container form", "{}", target.id());
    }
    Ok(())
}

/// The CTF lab's shape (`examples/invoice-portal-api`) in its VM form: each machine a Debian VM
/// that runs its own steps, the API's fixed host port forwarded as written, the launch token a
/// file delivered as a secret, never an environment value, and the setup run on the database.
#[test]
fn the_ctf_labs_vm_form_keeps_its_port_inputs_and_setup() -> Outcome {
    let lab = example("invoice-portal-api")?;
    assert_eq!(
        forms(&lab, Target::Vagrant)?,
        [("database".to_owned(), MachineShape::Vm), ("web".to_owned(), MachineShape::Vm)]
    );
    let vf = generated(&lab, Target::Vagrant, ".isoloom/vagrant/Vagrantfile")?;
    let web = between(&vf, "config.vm.define \"web\"", "\n  end\n")?;
    assert!(web.contains("name: \"provision/web.sh\""), "{web}");
    let forward = web.lines().find(|l| l.contains("\"forwarded_port\"")).ok_or("web forwards no port")?;
    assert!(
        forward.contains("guest: 80, host: ISOLOOM_HOSTPORTS.fetch(\"web/portal\", 8080)") && !forward.contains("auto_correct"),
        "the fixed port is never moved: {forward}"
    );
    assert!(!web.contains("CTF_"), "web receives none of the database's inputs:\n{web}");
    let database = between(&vf, "config.vm.define \"database\"", "\n  end\n")?;
    assert!(
        database.contains("install -D -o 0 -g 0 -m 0400 /var/lib/isoloom/secrets/CTF_LAUNCH_TOKEN '/run/lab/launch-token'"),
        "{database}"
    );
    assert!(
        !database.contains("\"CTF_LAUNCH_TOKEN\" =>"),
        "the token is never an environment value:\n{database}"
    );
    assert!(database.contains("sh '/opt/isoloom/build/database/evidence.sh'"), "{database}");
    Ok(())
}
