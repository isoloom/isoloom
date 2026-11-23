//! The provisioners of every example's VMs: run again safely, the readiness check last, the
//! checks installed.

use std::path::Path;

use crate::testing::{Outcome, plan, render, vagrantfile};

/// Every example Vagrant renders, with its Vagrantfile.
fn every_vagrantfile() -> Outcome<Vec<(String, String)>> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(examples)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        // An example Vagrant refuses (an appliance, a container only) has no Vagrantfile.
        if let Ok(file) = vagrantfile(&name) {
            out.push((name, file));
        }
    }
    out.sort();
    Ok(out)
}

/// Each `config.vm.define` of `file`: its name, and each of its provisioners as its name and its
/// statement, its script included: its own, in order, then its providers' (which Vagrant runs
/// after its own).
fn defines(file: &str) -> Vec<(String, Vec<(String, String)>)> {
    let r = isoloom_test_support::ruby_blocks(file);
    r.find("config.vm.define \"")
        .into_iter()
        .map(|define| {
            let name = define.head.split('"').nth(1).unwrap_or_default().to_owned();
            let provisioners = define
                .every_statement()
                .into_iter()
                .filter(|s| s.starts_with("m.vm.provision ") || s.starts_with("o.vm.provision "))
                .map(|s| {
                    let head = s.lines().next().unwrap_or_default();
                    let named = head.split("name: \"").nth(1).or_else(|| head.split(".vm.provision \"").nth(1));
                    (named.unwrap_or_default().split('"').next().unwrap_or_default().to_owned(), s.to_owned())
                })
                .collect();
            (name, provisioners)
        })
        .collect()
}

/// The provisioners that converge, run again by `vagrant provision`: each writes the same state
/// again (`>`, `replace`, a line added once, a unit rewritten), waits, checks, or is skipped by
/// its own mark (`setup`). A provisioner named otherwise (a project script, a shared volume) must
/// run once, behind a mark ([`once`](isoloom_vagrant) guards) or its `creates`.
const CONVERGE: &[&str] = &[
    "addresses",
    "resolver",
    "constraints",
    "disk",
    "data disks",
    "settings",
    "hosts",
    "routes",
    "default route",
    "forwarding",
    "ipv6",
    "trunks",
    "tc",
    "volumes",
    "project",
    "certificate authorities",
    "files",
    "secrets",
    "inputs",
    "configure",
    "before reboot",
    "reboot",
    "after reboot",
    "command",
    "setup",
    "no internet",
    "ready",
    "checks script",
    "checks",
    "router",
    "toolbox",
    "controller",
    "ansible",
    "online",
    "halt",
    "environment network",
];

#[test]
fn vagrant_provision_runs_again_safely() -> Outcome {
    let files = every_vagrantfile()?;
    assert!(files.len() > 10, "the examples render: {}", files.len());
    let mut once = 0;
    for (example, file) in &files {
        for (define, provisioners) in defines(file) {
            for (name, text) in provisioners {
                let marked = text.contains("isoloom_sum=") || text.contains("$isoloomSum");
                let created = text.contains("[ -e '") || text.contains("if (-not (Test-Path -LiteralPath");
                let windows_routes = name == "routes" && text.contains("route -p add");
                if windows_routes || !(CONVERGE.contains(&name.as_str()) || name.starts_with("wait for ")) {
                    assert!(marked || created, "{example}/{define}: {name} runs again on every provisioning:\n{text}");
                    once += 1;
                }
            }
        }
    }
    assert!(once > 10, "the project scripts and shared volumes run once: {once}");
    Ok(())
}

/// The readiness check is the last of each machine's own provisioners (`m.vm.provision`), and an
/// unhealthy machine fails it. A provider's own provisioner (`o.vm.provision`, QEMU's Windows
/// address) is not among them: Vagrant runs it after every one of the machine's own.
#[test]
fn the_readiness_check_is_each_machines_last_provisioner_and_its_failure_fails_vagrant_up() -> Outcome {
    for (example, file) in every_vagrantfile()? {
        let env = plan(&example)?;
        for (define, provisioners) in defines(&file) {
            if !env.machines.contains_key(&define) {
                continue;
            }
            let own: Vec<&(String, String)> = provisioners.iter().filter(|(_, s)| s.starts_with("m.vm.provision ")).collect();
            let (last, text) = own.last().ok_or(format!("{example}/{define}: no provisioner"))?;
            assert_eq!(last, "ready", "{example}/{define}");
            let healthcheck = env.machines.get(&define).is_some_and(|m| m.process.health.is_some());
            if healthcheck {
                assert!(
                    text.contains("exit 1") || text.contains("throw "),
                    "{example}/{define}: an unhealthy machine fails its provisioner:\n{text}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn each_vm_with_checks_has_them_installed_for_vagrant_ssh() -> Outcome {
    for example in ["segmented", "corp-ad-basics", "ansible-pair"] {
        let file = vagrantfile(example)?;
        let runners: Vec<String> = render(example)?
            .into_iter()
            .filter_map(|f| f.path.strip_prefix(".isoloom/vagrant/checks/")?.strip_suffix(".sh").map(str::to_owned))
            .collect();
        assert!(!runners.is_empty(), "{example} has checks");
        let header: String = file.lines().take_while(|l| l.starts_with('#')).collect::<Vec<_>>().join("\n");
        assert!(
            header.contains("# Checks: vagrant ssh <vm> -c 'sudo sh /var/lib/isoloom/checks.sh', for each of"),
            "{example}: {header}"
        );
        assert!(!header.contains("run.json") && !header.contains("runner"), "{example}: {header}");
        for (define, provisioners) in defines(&file) {
            let names: Vec<&str> = provisioners.iter().map(|(n, _)| n.as_str()).collect();
            let has = names.contains(&"checks script");
            assert_eq!(has, runners.contains(&define), "{example}/{define}: {names:?}");
            if has {
                assert!(header.contains(&define), "{example}: {define} in the header: {header}");
                let (_, text) = provisioners.iter().find(|(n, _)| n == "checks script").ok_or("checks script")?;
                assert!(
                    text.contains(&format!("File.read(File.join(__dir__, \"checks/{define}.sh\"))"))
                        && text.contains("mv -f /var/lib/isoloom/checks.sh.new /var/lib/isoloom/checks.sh")
                        && !text.contains("run: \"never\""),
                    "installed at every provisioning: {text}"
                );
            }
        }
    }
    Ok(())
}
