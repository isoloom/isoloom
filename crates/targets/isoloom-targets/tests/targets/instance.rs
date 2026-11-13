//! Instances: a spec several times on one host, each with its own names, Docker blocks,
//! published ports and output folder.

use isoloom_spec::{Target, instance};
use isoloom_targets::{generate, generate_instance};

use crate::testing::{Outcome, example};

#[test]
fn an_instance_gets_its_own_name_blocks_and_ports() -> Outcome {
    let spec = example("hello-stack")?.spec();
    let two = instance::apply(&spec, 2).unwrap();
    assert_eq!(two.name, "hello-stack-2");
    // The Docker block moves by the instance number in the second octet; the spec's own
    // addresses (what VMs use) stay as written.
    assert_eq!(two.networks["app"].docker.as_ref().unwrap().cidr, "10.62.0.0/24");
    assert_eq!(two.networks["app"].cidr, "10.60.0.0/24");
    assert_eq!(two.machines["web"].ports[0].publish, Some(8280));
    assert!(instance::apply(&spec, 0).is_err());
    assert!(instance::apply(&spec, 100).is_err());
    let mut high = spec.clone();
    high.machines["web"].ports[0].publish = Some(65500);
    assert!(instance::apply(&high, 1).unwrap_err().to_string().contains("65500"));
    Ok(())
}

#[test]
fn instance_files_live_next_to_the_committed_ones() -> Outcome {
    let spec = example("segmented")?;
    let files = generate_instance(&spec, Target::Docker, 3).unwrap();
    let compose = files
        .iter()
        .find(|f| f.path == ".isoloom-3/docker/compose.yml")
        .expect("compose under .isoloom-3/");
    assert!(compose.contents.contains("name: segmented-3"), "{}", compose.contents);
    // Every network moved: 10.61.x -> 10.64.x, machines keeping their last octets.
    assert!(
        compose.contents.contains("subnet: 10.64.10.0/24") && compose.contents.contains("ipv4_address: 10.64.10.10"),
        "{}",
        compose.contents
    );
    assert!(
        !compose.contents.contains("ipv4_address: 10.61.") && !compose.contents.contains("subnet: 10.61."),
        "{}",
        compose.contents
    );
    // Paths inside the files follow the folder.
    assert!(compose.contents.contains("# Checks: the runner (run.json), or by hand"));
    assert!(
        files.iter().all(|f| f.path.starts_with(".isoloom-3/")),
        "{:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    let snap = files.iter().find(|f| f.path == ".isoloom-3/resolved.json").unwrap();
    assert!(snap.contents.contains("\"instance\": 3"));
    assert!(snap.contents.contains("\"name\": \"segmented-3\""));
    // The committed files are untouched by instances.
    let plain = generate(&spec, Target::Docker).unwrap();
    assert!(plain.iter().any(|f| f.path == ".isoloom/docker/compose.yml"));
    Ok(())
}

#[test]
fn vagrant_instances_differ_by_name_only() -> Outcome {
    let spec = example("hello-stack")?;
    let files = generate_instance(&spec, Target::Vagrant, 1).unwrap();
    let vf = &files.iter().find(|f| f.path == ".isoloom-1/vagrant/Vagrantfile").unwrap().contents;
    // VM labels and internal network names carry the instance; addresses don't move.
    assert!(vf.contains("isoloom-hello-stack-1-app") && vf.contains("hello-stack-1 - web"), "{vf}");
    assert!(vf.contains("10.60.0.10"));
    // The runner picks each forward a free host port; the per-instance shift is the default it
    // falls back to, so two instances can both forward even without a runner choosing.
    assert!(vf.contains("guest: 80, host: ISOLOOM_HOSTPORTS.fetch(") && vf.contains(", 8180)"), "{vf}");
    // The project copy leaves every instance folder behind.
    assert!(vf.contains("IsoloomProject::COPY_RULES = \"/.isoloom*\n"), "{vf}");
    Ok(())
}

#[test]
fn the_snapshot_of_an_instance_that_cant_be_made_is_an_error_not_the_lab_as_written() -> Outcome {
    use isoloom_targets::resolved::resolve_with;
    let lab = example("hello-stack")?;
    assert_eq!(resolve_with(&lab, Some(2))?["instance"], 2);
    // Instance 100 doesn't exist: the snapshot isn't the lab as written saying "instance": 100.
    let refused = resolve_with(&lab, Some(100)).unwrap_err().to_string();
    assert!(refused.starts_with("instance 100:"), "{refused}");
    assert_eq!(resolve_with(&lab, None)?["instance"], serde_json::Value::Null);
    Ok(())
}

#[test]
fn an_instance_that_cant_be_made_is_its_own_error_not_the_target_refusing_the_lab() -> Outcome {
    use isoloom_targets::{Error, GenerateError};
    let lab = example("hello-stack")?;
    for n in [0, 100] {
        let Err(Error::Generate(e)) = generate_instance(&lab, Target::Docker, n) else {
            panic!("instance {n} generated");
        };
        assert!(
            matches!(e, GenerateError::Instance { target: Target::Docker, number, .. } if number == n),
            "{e:?}"
        );
        assert!(!e.is_refusal(), "{e}");
        assert!(e.to_string().starts_with(&format!("docker: instance {n}: ")), "{e}");
    }
    Ok(())
}

/// The names a VM target's files give on the host for instance `n` of the environment `lab`
/// (none: the environment as written): each VM's under every provider and each network's (the run manifest's
/// `vm_names` and networks' `native`), and the libvirt prefix and internal networks the
/// Vagrantfile sets. Each manifest name is checked to be the Vagrantfile's.
fn host_names(lab: &isoloom_targets::Lab, target: Target, n: Option<u8>) -> Outcome<std::collections::BTreeSet<String>> {
    let files = match n {
        Some(n) => generate_instance(lab, target, n)?,
        None => generate(lab, target)?,
    };
    let vagrantfile = crate::testing::file(&files, "/Vagrantfile")?;
    let mut names = std::collections::BTreeSet::new();
    let prefix = vagrantfile
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("v.default_prefix = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or("the Vagrantfile sets a libvirt prefix")?;
    names.insert(format!("prefix {prefix}"));
    for name in vagrantfile.split("virtualbox__intnet: \"").skip(1).filter_map(|rest| rest.split('"').next()) {
        names.insert(format!("network {name}"));
    }
    // The hybrid target writes no run manifest.
    let Some(run) = files.iter().find(|f| f.path.ends_with("/run.json")) else {
        return Ok(names);
    };
    let manifest: serde_json::Value = serde_json::from_str(&run.contents)?;
    let list = |v: Option<&serde_json::Value>| v.and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    for machine in list(manifest.get("machines")) {
        let vm_names = machine
            .get("native")
            .and_then(|native| native.get("vm_names"))
            .and_then(serde_json::Value::as_object);
        for (provider, name) in vm_names.into_iter().flatten() {
            let name = name.as_str().ok_or("a VM name is text")?;
            if provider == "libvirt" {
                assert!(name.starts_with(prefix), "{}: {name} has the prefix {prefix}", target.id());
            } else {
                assert!(vagrantfile.contains(&format!("\"{name}\"")), "{}: the Vagrantfile names {name}", target.id());
            }
            names.insert(format!("{provider} {name}"));
        }
    }
    if target != Target::DockerVm {
        // The docker-vm target's networks are Docker's, inside its one VM.
        for net in list(manifest.get("networks")) {
            let name = net.get("native").and_then(serde_json::Value::as_str).ok_or("a network's host name is text")?;
            assert!(
                names.contains(&format!("network {name}")),
                "{}: the Vagrantfile makes network {name}",
                target.id()
            );
        }
    }
    Ok(names)
}

#[test]
fn two_instances_never_share_a_vm_network_or_prefix_name_and_the_environment_as_written_keeps_its_own() -> Outcome {
    let mut compared = 0;
    for name in ["solo-web", "segmented", "corp-ad-basics", "hello-stack", "configured-windows", "mixed-office"] {
        let lab = example(name)?;
        for target in [Target::Vagrant, Target::DockerVm, Target::Hybrid] {
            if !lab.possible().contains(&target) || isoloom_targets::refusal(&lab, target).is_some() {
                continue;
            }
            let plain = host_names(&lab, target, None)?;
            let (one, two) = (host_names(&lab, target, Some(1))?, host_names(&lab, target, Some(2))?);
            for (a, b) in [(&plain, &one), (&plain, &two), (&one, &two)] {
                assert!(
                    a.is_disjoint(b),
                    "{name} on {}: shared {:?}",
                    target.id(),
                    a.intersection(b).collect::<Vec<_>>()
                );
            }
            // The environment as written keeps the names it always had.
            let env = &lab.spec().name;
            assert!(plain.contains(&format!("prefix {env}_")), "{name} on {}: {plain:?}", target.id());
            assert!(one.contains(&format!("prefix {env}-1_")), "{name} on {}: {one:?}", target.id());
            compared += 1;
        }
    }
    assert!(compared >= 6, "{compared} labs and targets compared");
    Ok(())
}
