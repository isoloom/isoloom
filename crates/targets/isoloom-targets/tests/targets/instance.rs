//! Instances: a spec several times on one host, each with its own names, Docker blocks,
//! published ports and output folder.

use isoloom_spec::Target;
use isoloom_targets::{generate, generate_instance};
use isoloom_test_support::{assert_every, ruby_blocks, yaml, yaml_at, yaml_keys_at, yaml_str};

use crate::testing::{Outcome, define, example, file, header};

#[test]
fn instance_files_live_next_to_the_committed_ones() -> Outcome {
    let spec = example("segmented")?;
    let files = generate_instance(&spec, Target::Docker, 3)?;
    let text = file(&files, ".isoloom-3/docker/compose.yml")?;
    let compose = yaml(text);
    assert_eq!(yaml_str(&compose, "name"), "segmented-3");
    // Every network moved: 10.61.x -> 10.64.x, machines keeping their last octets.
    assert_eq!(yaml_str(&compose, "networks.front.ipam.config.0.subnet"), "10.64.10.0/24");
    assert_eq!(yaml_str(&compose, "services.web.networks.front.ipv4_address"), "10.64.10.10");
    let subnets: Vec<String> = yaml_keys_at(&compose, "networks")
        .iter()
        .filter_map(|n| yaml_at(&compose, &format!("networks.{n}.ipam.config.0.subnet")).and_then(serde_yaml_ng::Value::as_str))
        .map(str::to_owned)
        .collect();
    assert_every!(&subnets, |s: &&String| s.starts_with("10.64."));
    // Paths inside the files follow the folder.
    assert!(
        header(text)
            .iter()
            .any(|l| l.starts_with("Checks: docker compose -f .isoloom-3/docker/compose.yml --profile check run --rm")),
        "{text}"
    );
    assert_every!(&files, |f: &&isoloom_targets::GeneratedFile| f.path.starts_with(".isoloom-3/"));
    let snap: serde_json::Value = serde_json::from_str(file(&files, ".isoloom-3/resolved.json")?)?;
    assert_eq!((&snap["instance"], &snap["name"]), (&serde_json::json!(3), &serde_json::json!("segmented-3")));
    // The committed files are untouched by instances.
    let plain = generate(&spec, Target::Docker)?;
    assert!(plain.iter().any(|f| f.path == ".isoloom/docker/compose.yml"));
    Ok(())
}

#[test]
fn vagrant_instances_differ_by_name_only() -> Outcome {
    let spec = example("hello-stack")?;
    let files = generate_instance(&spec, Target::Vagrant, 1)?;
    let vf = ruby_blocks(file(&files, ".isoloom-1/vagrant/Vagrantfile")?);
    let web = define(&vf, "web");
    // VM labels and internal network names carry the instance; addresses don't move.
    let nics = web.statements("m.vm.network \"private_network\"");
    assert!(
        matches!(nics.as_slice(), [one] if one.contains("ip: \"10.60.0.10\"") && one.contains("virtualbox__intnet: \"isoloom-hello-stack-1-#{ISOLOOM_KEY}-app\"")),
        "{nics:?}"
    );
    assert!(web.every_statement().contains(&"v.name = \"hello-stack-1-#{ISOLOOM_KEY} - web\""), "{web:?}");
    // The runner picks each forward a free host port; the per-instance shift is the default it
    // falls back to, so two instances can both forward even without a runner choosing.
    let forwards = web.statements("m.vm.network \"forwarded_port\", guest: 80,");
    assert!(
        matches!(forwards.as_slice(), [one] if one.contains("host: ENV.fetch(\"ISOTHRUM_HOST_PORT_WEB_80_TCP\", ISOLOOM_HOSTPORTS.fetch(") && one.contains(", 8180)).to_i")),
        "{forwards:?}"
    );
    // The project copy leaves every instance folder behind.
    let rules: Vec<&str> = vf
        .every_statement()
        .into_iter()
        .filter(|s| s.starts_with("IsoloomProject::COPY_RULES = "))
        .collect();
    assert!(
        matches!(rules.as_slice(), [one] if one.starts_with("IsoloomProject::COPY_RULES = \"/.isoloom*\n")),
        "{rules:?}"
    );
    Ok(())
}

#[test]
fn the_snapshot_of_an_instance_that_cant_be_made_is_an_error_not_the_lab_as_written() -> Outcome {
    use isoloom_targets::resolved::resolve_with;
    let lab = example("hello-stack")?;
    assert_eq!(resolve_with(&lab, Some(2))?["instance"], 2);
    // Instance 100 doesn't exist: the snapshot isn't the lab as written saying "instance": 100.
    let refused = resolve_with(&lab, Some(100)).err().ok_or("instance 100 resolved")?.to_string();
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

/// The names a VM target's Vagrantfile gives on the host for instance `n` of the environment
/// `lab` (none: the environment as written), as written in it, the copy's key still to read
/// (`#{ISOLOOM_KEY}`): the libvirt prefix and the internal networks. Each holds the copy's
/// key, which only the Vagrantfile reads.
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
        assert!(name.contains("#{ISOLOOM_KEY}"), "{}: network {name} holds the copy's key", target.id());
        names.insert(format!("network {name}"));
    }
    Ok(names)
}

#[test]
fn two_instances_never_share_a_vm_network_or_prefix_name() -> Outcome {
    let mut compared = 0;
    for name in ["solo-web", "segmented", "corp-ad-basics", "hello-stack", "configured-windows", "mixed-office"] {
        let lab = example(name)?;
        for target in [Target::Vagrant] {
            if !lab.possible().contains(&target) || isoloom_targets::refusal(&lab, target).unwrap().is_some() {
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
            // The environment's name, its instance's, then the copy's key.
            let env = &lab.spec().name;
            assert!(
                plain.contains(&format!("prefix {env}-#{{ISOLOOM_KEY}}_")),
                "{name} on {}: {plain:?}",
                target.id()
            );
            assert!(one.contains(&format!("prefix {env}-1-#{{ISOLOOM_KEY}}_")), "{name} on {}: {one:?}", target.id());
            compared += 1;
        }
    }
    assert!(compared >= 6, "{compared} labs and targets compared");
    Ok(())
}

/// An instance's Vagrantfile stops `vagrant up` before any VM is made on the providers whose
/// networks are this computer's, one per block, which the environment as written makes too; the
/// environment as written stops nowhere.
#[test]
fn an_instances_vagrantfile_stops_where_its_networks_would_be_the_environments() -> Outcome {
    let lab = crate::testing::example("hello-stack")?;
    let stop = |vf: &isoloom_test_support::RubyBlock| -> Vec<String> {
        vf.find("config.trigger.before :up")
            .iter()
            .flat_map(|t| t.every_statement())
            .map(str::to_owned)
            .collect()
    };
    let as_written = crate::testing::vagrantfile(&lab)?;
    assert!(stop(&as_written).is_empty(), "{:?}", stop(&as_written));
    let files = isoloom_targets::generate_instance(&lab, Target::Vagrant, 3)?;
    let instance = isoloom_test_support::ruby_blocks(crate::testing::file(&files, ".isoloom-3/vagrant/Vagrantfile")?);
    let stops = stop(&instance);
    assert!(
        stops.iter().any(|s| s.starts_with("abort(\"#{machine.name}: \" + \"instance 3's networks")
            && s.ends_with(" if [:vmware_desktop, :parallels, :libvirt].include?(machine.provider_name)")),
        "{stops:?}"
    );
    Ok(())
}
