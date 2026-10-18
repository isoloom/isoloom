//! What a target refuses and why: the field that rules it out, a spec nobody validated refused
//! without a panic.

use isoloom_spec::Target;
use isoloom_targets::{Error, GenerateError, generate, generate_all};

use crate::testing::{Outcome, example, lab};

#[test]
fn a_lab_every_generator_refuses_is_refused_with_reasons() -> Outcome {
    // Windows 11 narrowed to the clouds, where none has an image of it yet.
    let spec =
        lab("version: 1\nname: w\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\ntargets: [cloud-vm]\nmachines:\n  ws: { os: windows@11, networks: { lab: 10 } }\n")?;
    let (files, skipped) = generate_all(&spec);
    assert_eq!(files.iter().map(|f| &f.path).collect::<Vec<_>>(), Vec::<&String>::new());
    assert_ne!(skipped, Vec::new());
    Ok(())
}

#[test]
fn refusal_reports_what_a_generator_would_refuse() -> Outcome {
    // A target can be possible by its machines' forms yet refused by its generator; `targets`
    // used to show ✓ for those (the clouds for a Windows 11 workstation, which none has an
    // image of yet).
    let ad = example("corp-ad-basics")?;
    let why = isoloom_targets::refusal(&ad, Target::CloudVm).ok_or("no cloud has a Windows 11 image")?;
    assert!(why.contains("no AWS image for `windows-11`"), "{why}");
    // A Windows lab of PowerShell steps clones a template on Proxmox.
    let windows = example("windows-hello")?;
    assert_eq!(isoloom_targets::refusal(&windows, Target::Proxmox), None);
    // Environment-level provisioning runs from a controller on Proxmox now, so it is not refused.
    let ansible = example("ansible-pair")?;
    assert_eq!(isoloom_targets::refusal(&ansible, Target::Proxmox), None);
    // A plain Linux lab is generated: nothing to refuse.
    let plain = example("hello-stack")?;
    assert_eq!(isoloom_targets::refusal(&plain, Target::Proxmox), None);
    assert_eq!(isoloom_targets::refusal(&plain, Target::Docker), None);
    Ok(())
}

const BAD_CIDR: &str = "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/33 }\nmachines:\n  web: { networks: { app: 10 }, app: { image: nginx } }\n";

const UNDECLARED_NETWORK: &str =
    "version: 1\nname: t\nnetworks:\n  app: { cidr: 10.20.0.0/24 }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n";

/// That `refusal` on a spec nobody validated refuses every target with the spec's problem, and
/// doesn't panic.
fn refused_everywhere(yaml: &str, problem: &str) -> Outcome {
    let spec = lab(yaml)?;
    for t in Target::ALL {
        let why = isoloom_targets::refusal(&spec, t).ok_or_else(|| format!("{t:?} not refused"))?;
        assert!(why.contains(problem), "{t:?}: {why}");
    }
    Ok(())
}

#[test]
fn refusal_of_an_invalid_cidr_does_not_panic() -> Outcome {
    refused_everywhere(BAD_CIDR, "networks.app.cidr")
}

#[test]
fn refusal_of_an_undeclared_network_does_not_panic() -> Outcome {
    refused_everywhere(UNDECLARED_NETWORK, "no network named `lan`")
}

#[test]
fn checks_plan_of_an_undeclared_network_does_not_panic() -> Outcome {
    let planned = isoloom_plan::checks::plan(&lab(UNDECLARED_NETWORK)?.spec());
    assert!(planned.iter().all(|c| !c.name.is_empty()), "{planned:?}");
    Ok(())
}

#[test]
fn unsupported_features_are_refused_with_the_reason() -> Outcome {
    let ad = example("corp-ad-basics")?;
    // Windows has no container form: the Docker target refuses it at the field that rules it out.
    assert!(matches!(
        generate(&ad, Target::Docker),
        Err(Error::Refused(GenerateError::Unsupported { target: Target::Docker, ref what }))
            if what == "machines.dc01.os: Windows has no container form in 1.0"
    ));
    // The domain's playbooks run from the controller: the VM targets generate it.
    assert!(generate(&ad, Target::Vagrant).is_ok() && generate(&ad, Target::Proxmox).is_ok());
    assert!(
        matches!(generate(&ad, Target::CloudVm), Err(Error::Refused(GenerateError::Unsupported { ref what, .. })) if what.contains("no AWS image for `windows-11`"))
    );
    Ok(())
}

#[test]
fn an_arm64_access_machine_that_is_actually_built_is_still_refused_on_the_cloud() -> Outcome {
    // An access machine with no implementation isn't instantiated, so its arch is moot. One that
    // declares a VM is built, so arm64 must still be caught (the cloud images are x86-64 only).
    let spec = lab(
        "version: 1\nname: arm-access\nnetworks:\n  lab: { cidr: 10.60.0.0/24 }\nmachines:\n  kali:\n    os: debian@12\n    networks: { lab: 10 }\n    access: true\n    hardware: { arch: arm64 }\n    app: { steps: [{ run: p.sh }] }\n",
    )?;
    assert!(
        matches!(generate(&spec, Target::CloudVm), Err(Error::Refused(GenerateError::Unsupported { ref what, .. })) if what.contains("arm64")),
        "an arm64 access machine with a VM must not slip past the cloud refusal"
    );
    Ok(())
}

#[test]
fn windows_refuses_what_it_cant_do_yet() -> Outcome {
    let base = "version: 1\nname: t\nnetworks:\n  lab: { cidr: 10.9.0.0/24 }\nmachines:\n  NAME:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    app: { steps: [STEP] }\n";
    for (name, step, why) in [
        ("dc01", "{ playbook: setup.yml }", "PowerShell scripts"),
        ("averyveryverylongname", "{ run: a.ps1 }", "15 characters"),
    ] {
        let spec = lab(&base.replace("NAME", name).replace("STEP", step))?;
        let err = generate(&spec, Target::Vagrant).unwrap_err().to_string();
        assert!(err.contains(why), "{name}: {err}");
    }
    Ok(())
}
