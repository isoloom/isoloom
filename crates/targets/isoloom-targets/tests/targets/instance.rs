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
        matches!(forwards.as_slice(), [one] if one.contains("host: ENV.fetch(\"HOST_PORT_WEB_80_TCP\", ISOLOOM_HOSTPORTS.fetch(") && one.contains(", 8180)).to_i")),
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

/// The paths under `folder` (`.isoloom/`, `.isoloom-<n>/`) that `files` name in their texts, each
/// with the file that names it, which none of `files` is (nor holds, for a folder).
fn missing(files: &[isoloom_targets::GeneratedFile], folder: &str) -> Vec<String> {
    let written = |p: &str| {
        files
            .iter()
            .any(|f| f.path == p || f.path.starts_with(&format!("{}/", p.trim_end_matches('/'))))
    };
    let mut out = Vec::new();
    for f in files {
        for (at, _) in f.contents.match_indices(folder) {
            let (before, rest) = f.contents.split_at(at);
            // A path in the text's own boundary: the folder starts it (not `x.isoloom/`, nor a
            // folder nested in a longer path, checked whole from its own start), and it ends
            // where a quote, a space, a separator or an interpolation does. A placeholder in a
            // comment (`checks/isoloom-exec-<machine>.sh`) names no one file.
            if before.chars().next_back().is_some_and(|c| c.is_alphanumeric() || "./-_".contains(c)) {
                continue;
            }
            let end = rest.find(|c: char| c.is_whitespace() || "\"'`,;:)]}{$\\|".contains(c)).unwrap_or(rest.len());
            let path = rest.split_at(end).0.trim_end_matches('.');
            if !path.contains('<') && !written(path) {
                out.push(format!("{} names {path}", f.path));
            }
        }
    }
    out
}

/// Every path one of an instance's files names in its folder is one of its files, as it is
/// without an instance: what a Dockerfile copies, a Compose file builds or mounts, a check runs.
fn paths_hold(lab: &isoloom_targets::Lab, targets: &[Target], what: &str) -> Outcome<Vec<String>> {
    let mut broken = Vec::new();
    for target in lab.possible().into_iter().filter(|t| targets.contains(t)) {
        if isoloom_targets::refusal(lab, target)?.is_some() {
            continue;
        }
        for (files, folder) in [(generate(lab, target)?, ".isoloom/"), (generate_instance(lab, target, 7)?, ".isoloom-7/")] {
            broken.extend(
                files
                    .iter()
                    .filter(|f| !f.path.starts_with(folder))
                    .map(|f| format!("{what}: {}: {} is outside {folder}", target.id(), f.path)),
            );
            broken.extend(missing(&files, folder).into_iter().map(|m| format!("{what}: {}: {m}", target.id())));
        }
    }
    Ok(broken)
}

#[test]
fn every_path_an_examples_instance_names_is_one_it_writes() -> Outcome {
    let mut broken = Vec::new();
    for dir in crate::every_example::example_dirs()? {
        let lab = isoloom_targets::Lab::new(isoloom_spec::load(&dir)?);
        broken.extend(paths_hold(&lab, &Target::ALL, &dir.file_name().unwrap_or_default().to_string_lossy())?);
    }
    assert!(broken.is_empty(), "{broken:#?}");
    Ok(())
}

#[test]
fn every_path_an_instance_of_a_catalogue_service_names_is_one_it_writes() -> Outcome {
    // Regression: a catalogue template, rendered for the image's build under a nested `.isoloom/`,
    // was written there while the Dockerfile copied it from `.isoloom-<n>/`: the build failed.
    use isoloom_catalogue::conformance::PROBE;
    let catalogue = isoloom_catalogue::Catalogue::embedded().map_err(ToString::to_string)?;
    let (mut broken, mut checked) = (Vec::new(), Vec::new());
    for entry in catalogue.entries() {
        // On each OS, the newest release of each source: the container form differs by both
        // (mysql@8.4 is its official image on Debian 12, 8.4.7 Oracle's build, on Ubuntu 22.04 too).
        let mut shapes = Vec::new();
        for (release, os) in entry.definition().releases.iter().rev().flat_map(|r| r.os.iter().map(move |os| (r, os))) {
            if shapes.contains(&(os, &release.source)) {
                continue;
            }
            shapes.push((os, &release.source));
            for case in entry.contract().cases.keys() {
                let written = entry.conformance_lab(catalogue, case, release, *os)?;
                // The access machine is the runner's to supply: here, the service machine's OS.
                let mut spec: serde_yaml_ng::Value = serde_yaml_ng::from_str(&written.spec)?;
                spec["machines"][PROBE]["os"] = os.to_string().into();
                let dir = tempfile::tempdir()?;
                for (path, text) in [("isoloom.yml".to_owned(), serde_yaml_ng::to_string(&spec)?)]
                    .into_iter()
                    .chain(written.project.clone())
                {
                    let at = dir.path().join(path);
                    std::fs::create_dir_all(at.parent().ok_or("a project file in no folder")?)?;
                    std::fs::write(at, text)?;
                }
                let lab = isoloom_targets::Lab::new(isoloom_spec::load(dir.path())?);
                // A case whose helpers don't run on this OS (php-fpm 8.2 on Debian 13) isn't a lab.
                if !crate::testing::problems(&lab).is_empty() {
                    continue;
                }
                let what = format!("{}@{} {case} on {os}", entry.name(), release.version);
                // The image's build is where the files are named (Kubernetes builds the same
                // Dockerfiles; a VM's set-up carries them inline).
                broken.extend(paths_hold(&lab, &[Target::Docker], &what)?);
                checked.push(what);
            }
        }
    }
    assert!(checked.iter().any(|c| c == "mysql@8.4.7 readonly on debian@12"), "{checked:?}");
    assert!(checked.iter().any(|c| c == "mysql@8.4 readonly on debian@12"), "{checked:?}");
    assert!(broken.is_empty(), "{broken:#?}");
    Ok(())
}

/// Instance 1 of `invoice-portal-api`, whose web machine publishes its portal on 8080, fixed (its
/// pages call that URL), on `target`: the file at `path` under `.isoloom-1/`, the snapshot giving
/// the portal's URL on 8080 too.
fn fixed_port_instance(target: Target, path: &str) -> Outcome<String> {
    let lab = example("invoice-portal-api")?;
    let files = generate_instance(&lab, target, 1)?;
    let snap: serde_json::Value = serde_json::from_str(file(&files, ".isoloom-1/resolved.json")?)?;
    assert_eq!(
        snap.pointer("/machines/web/ports/portal/url").and_then(serde_json::Value::as_str),
        Some("http://127.0.0.1:8080/"),
        "{target:?}"
    );
    Ok(file(&files, &format!(".isoloom-1/{path}"))?.to_owned())
}

#[test]
fn an_instance_publishes_a_fixed_port_on_its_own_number_on_compose() -> Outcome {
    // Regression: instance 1 published it on 8180, where the portal's own pages don't call it.
    let compose = yaml(&fixed_port_instance(Target::Docker, "docker/compose.yml")?);
    assert_eq!(yaml_str(&compose, "services.web.ports.0"), "${ISOLOOM_PUBLISH_ADDRESS:-127.0.0.1}:8080:80");
    Ok(())
}

#[test]
fn an_instance_forwards_a_fixed_port_from_its_own_number_on_vagrant() -> Outcome {
    let vf = ruby_blocks(&fixed_port_instance(Target::Vagrant, "vagrant/Vagrantfile")?);
    let forwards = define(&vf, "web").statements("m.vm.network \"forwarded_port\", guest: 80,");
    assert!(
        matches!(forwards.as_slice(), [one] if one.contains("host: ISOLOOM_HOSTPORTS.fetch(\"web/portal\", 8080)")),
        "{forwards:?}"
    );
    Ok(())
}

#[test]
fn an_instance_publishes_a_fixed_port_on_its_own_number_on_terraform() -> Outcome {
    let proxmox = fixed_port_instance(Target::Proxmox, "proxmox/main.tf")?;
    assert!(proxmox.contains("value = { \"web/portal\" = \"${local.router_address}:8080\" }"), "{proxmox}");
    let aws = fixed_port_instance(Target::CloudVm, "cloud-vm/aws/main.tf")?;
    assert!(aws.contains("value = { \"web/portal\" = \"${aws_instance.web.public_ip}:8080\" }"), "{aws}");
    Ok(())
}
