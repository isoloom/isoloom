//! Windows machines: WinRM and PowerShell, on Vagrant and the clouds, and Linux machines beside
//! them.

use isoloom_spec::Target;
use isoloom_targets::generate;

use crate::testing::{Outcome, between, example, from, generated, lab};

#[test]
fn windows_machines_use_winrm_and_powershell() -> Outcome {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app:\n      steps: [{ run: provision/dc.ps1 }]\n  web:\n    os: windows-server@2019\n    networks: { lab: 20 }\n    depends_on: [dc01]\n    app:\n      steps: [{ run: provision/web.ps1 }]\noverrides:\n  vagrant:\n    machines:\n      web: { box: example/win2019, box_version: '1.0' }\n",
    )?;
    assert_eq!(spec.problems(), vec![]);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let dc = between(vf, "config.vm.define \"dc01\"", "config.vm.define \"web\"")?;
    assert!(dc.contains("m.vm.box = \"StefanScherer/windows_2019\""));
    assert!(dc.contains("m.vm.box_version = \"2021.05.15\""));
    assert!(dc.contains("m.vm.communicator = \"winrm\""));
    assert!(dc.contains("drivers\\\\etc\\\\hosts"), "the Windows hosts file: {dc}");
    // The project in C:\isoloom first, then each step run from there.
    let project = dc.find("m.vm.provision \"isoloom_project\", name: \"project\"").expect("the project step");
    let step = dc
        .find("name: \"provision/dc.ps1\", inline: \"Set-Location C:\\\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'provision/dc.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }\"")
        .expect("the step, from the project");
    assert!(project < step);
    let web = from(vf, "config.vm.define \"web\"")?;
    assert!(web.contains("m.vm.box = \"example/win2019\""), "the spec's image wins");
    assert!(web.contains("m.vm.box_version = \"1.0\""));
    assert!(web.contains("Test-NetConnection 'dc01' -Port $p"));
    Ok(())
}

#[test]
fn cloud_vm_runs_windows_over_winrm_with_its_name() -> Outcome {
    let spec = example("windows-hello")?;
    let tf = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    // Amazon's image, no key pair (AWS refuses ED25519 on Windows), WinRM with a generated password.
    assert!(tf.contains("Windows_Server-2019-English-Full-Base-*"));
    assert!(tf.contains("resource \"random_password\" \"windows\""));
    assert!(tf.contains("type     = \"winrm\""));
    // Renamed and restarted before its set-up, which checks the name.
    assert!(tf.contains("resource \"time_sleep\" \"web01_restart\""));
    assert!(tf.contains("still named"));
    // Auto-stop as a scheduled task (a pending shutdown would block the rename's restart).
    assert!(tf.contains("isoloom-auto-stop"));
    // A Windows-only environment: the controller runs the checks.
    assert!(tf.contains("host     = aws_eip.isoloom_controller.public_ip"), "{tf}");
    Ok(())
}

#[test]
fn linux_machines_leave_windows_out_of_etc_hosts() -> Outcome {
    // A Linux member joining an AD domain must not have the Windows DC in /etc/hosts by its
    // short name: that shadows AD DNS and breaks the Kerberos SPN lookup on realm join.
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app: { steps: [{ run: p.ps1 }] }\n  lx01:\n    os: ubuntu@24.04\n    networks: { lab: 12 }\n    ports: { ssh: 22 }\n    depends_on: [dc01]\n    app: { steps: [{ run: p.sh }] }\n",
    )?;
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let lx01 = from(vf, "config.vm.define \"lx01\"")?;
    let (head, rest) = lx01.split_at_checked(20).ok_or("a short define")?;
    let lx = format!("{head}{}", rest.split("config.vm.define").next().unwrap_or_default());
    // The Windows DC isn't added to lx01's /etc/hosts by name.
    assert!(
        !lx.contains("192.168.56.10 dc01"),
        "Windows DC should be left out of the Linux host's /etc/hosts"
    );
    // And the depends_on wait on the Windows DC is by address, not name.
    assert!(lx.contains("/192.168.56.10/389"), "the Windows dependency is waited on by address: {lx}");
    assert!(!lx.contains("</dev/tcp/dc01/"), "not by the name that isn't in /etc/hosts");
    Ok(())
}
