//! Windows machines: WinRM and PowerShell, on Vagrant and the clouds, and Linux machines beside
//! them.

use isoloom_spec::Target;
use isoloom_targets::generate;

use isoloom_test_support::{hcl_blocks, hcl_body, hcl_literals, hcl_names, hcl_nested, hcl_text, ruby_blocks};

use crate::testing::{Outcome, define, example, generated, lab, provisioners};

#[test]
fn windows_machines_use_winrm_and_powershell() -> Outcome {
    let spec = lab(
        "version: 1\nname: t\nnetworks:\n  lab: { cidr: 192.168.56.0/24 }\nmachines:\n  dc01:\n    os: windows-server@2019\n    networks: { lab: 10 }\n    ports: { ldap: 389 }\n    app:\n      steps: [{ run: provision/dc.ps1 }]\n  web:\n    os: windows-server@2019\n    networks: { lab: 20 }\n    depends_on: [dc01]\n    app:\n      steps: [{ run: provision/web.ps1 }]\noverrides:\n  vagrant:\n    machines:\n      web: { box: example/win2019, box_version: '1.0' }\n",
    )?;
    assert_eq!(spec.problems(), vec![]);
    let vf = &generate(&spec, Target::Vagrant).unwrap()[0].contents;
    let file = ruby_blocks(vf);
    let dc01 = define(&file, "dc01");
    assert_eq!(dc01.statements("m.vm.box = "), ["m.vm.box = \"StefanScherer/windows_2019\""]);
    assert_eq!(dc01.statements("m.vm.box_version = "), ["m.vm.box_version = \"2021.05.15\""]);
    assert_eq!(dc01.statements("m.vm.communicator = "), ["m.vm.communicator = \"winrm\""]);
    let hosts = dc01.statements("m.vm.provision \"shell\", name: \"hosts\"");
    assert!(
        matches!(hosts.as_slice(), [one] if one.contains("drivers\\\\etc\\\\hosts")),
        "the Windows hosts file: {hosts:?}"
    );
    // The project in C:\isoloom first, then each step run from there.
    let steps = provisioners(dc01);
    let at = |name: &str| steps.iter().position(|s| s == name).ok_or_else(|| format!("no `{name}` step in {steps:?}"));
    assert!(at("project")? < at("provision/dc.ps1")?, "{steps:?}");
    // Run once, until the script changes (a mark of its hash).
    let step = dc01.statements("m.vm.provision \"shell\", name: \"provision/dc.ps1\", inline: \"$isoloomSum = (Get-FileHash");
    let line = step.first().and_then(|s| s.lines().next()).ok_or("the step, run once")?;
    assert!(
        line.contains(
            "Set-Location C:\\\\isoloom; & powershell -NoProfile -ExecutionPolicy Bypass -File 'provision/dc.ps1'; if ($LASTEXITCODE) { exit $LASTEXITCODE }"
        ),
        "the step, from the project: {line}"
    );
    let web = define(&file, "web");
    assert_eq!(web.statements("m.vm.box = "), ["m.vm.box = \"example/win2019\""], "the spec's image wins");
    assert_eq!(web.statements("m.vm.box_version = "), ["m.vm.box_version = \"1.0\""]);
    let waits = web.statements("m.vm.provision \"shell\", name: \"wait for dc01\"");
    assert!(
        matches!(waits.as_slice(), [one] if one.contains("Test-NetConnection 'dc01' -Port $p")),
        "{waits:?}"
    );
    Ok(())
}

#[test]
fn cloud_vm_runs_windows_over_winrm_with_its_name() -> Outcome {
    let spec = example("windows-hello")?;
    let tf = generated(&spec, Target::CloudVm, ".isoloom/cloud-vm/aws/main.tf")?;
    // Amazon's image, no key pair (AWS refuses ED25519 on Windows), WinRM with a generated password.
    let body = hcl_body(&tf);
    let images: Vec<String> = hcl_blocks(&body, "data", &["aws_ami"])
        .iter()
        .flat_map(|d| isoloom_test_support::hcl_values(d.body(), "values"))
        .collect();
    assert!(images.iter().any(|v| v == "Windows_Server-2019-English-Full-Base-*"), "{images:?}");
    assert_eq!(hcl_names(&body, "resource", "random_password"), ["windows"]);
    // Every connection Terraform opens: over WinRM to the Windows machine, else through the
    // controller (a Windows-only environment: the controller runs the checks).
    let connections: Vec<(String, String)> = body
        .blocks()
        .filter(|b| b.identifier() == "resource")
        .flat_map(|b| {
            hcl_nested(b.body(), "connection")
                .into_iter()
                .chain(b.body().blocks().flat_map(|p| hcl_nested(p.body(), "connection")))
        })
        .map(|c| (hcl_text(c.body(), "type"), hcl_text(c.body(), "host")))
        .collect();
    assert!(connections.iter().any(|(kind, _)| kind == "winrm"), "{connections:?}");
    assert!(
        connections.iter().any(|(_, host)| host == "aws_eip.isoloom_controller.public_ip"),
        "{connections:?}"
    );
    // Renamed and restarted before its set-up, which checks the name.
    assert_eq!(hcl_names(&body, "resource", "time_sleep"), ["web01_restart"]);
    let literals = hcl_literals(&body);
    assert!(literals.iter().any(|l| l.contains("still named")), "the set-up checks the name");
    // The run's end as a scheduled task (a pending shutdown would block the rename's restart).
    assert!(literals.iter().any(|l| l.contains("isoloom-expiry")), "the expiry's task");
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
    let file = ruby_blocks(vf);
    let lx = define(&file, "lx01").every_statement().join("\n");
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
