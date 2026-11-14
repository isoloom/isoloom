//! A spec reaches the computer that runs it only as far as whoever runs it allows: the fixtures
//! under `tests/fixtures/host-reach/` ask for the host in each way a spec can (a privileged
//! container, a device, folders outside the project or written to, the engine's socket, a VM's
//! boot files, settings and requirements for the host's own tools, links out of the project), and
//! each is refused, or allowed with `--allow-host`, or, for a value that only reads like syntax,
//! carried as text. Links are made by the test (a repository holds none).

use std::path::Path;

use crate::testing::{Outcome, Scratch, isoloom, project, scratch};

/// A fresh copy of fixture `name`.
fn fixture(name: &str) -> Outcome<Scratch> {
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/host-reach").join(name);
    let dir = scratch(name)?;
    copy(&from, &dir)?;
    Ok(dir)
}

/// Copies the folder `from` into `to`.
fn copy(from: &Path, to: &Path) -> Outcome {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The problems `isoloom validate <dir> --json <args>` reports (it fails), each `field: reason`.
fn refused(home: &Path, dir: &Path, args: &[&str]) -> Outcome<Vec<String>> {
    let mut all: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"validate", &dir, &"--json"];
    for a in args {
        all.push(a);
    }
    let run = isoloom(home, &all)?;
    assert_eq!(run.code, Some(1), "{run:?}");
    let report: serde_json::Value = serde_json::from_str(&run.stdout)?;
    let problems = report["problems"].as_array().ok_or("no problems")?;
    Ok(problems
        .iter()
        .map(|p| format!("{}: {}", p["path"].as_str().unwrap_or_default(), p["reason"].as_str().unwrap_or_default()))
        .collect())
}

/// Whether some line is at field `at` and says `says`.
fn has(found: &[String], at: &str, says: &str) -> bool {
    found.iter().any(|l| l.starts_with(&format!("{at}: ")) && l.contains(says))
}

#[cfg(unix)]
fn link(target: &str, at: &Path) -> Outcome {
    std::os::unix::fs::symlink(target, at)?;
    Ok(())
}

#[test]
#[cfg(unix)]
fn a_container_reaching_the_host_is_refused_until_each_access_is_allowed() -> Outcome {
    let home = scratch("host-reach-home")?;
    let dir = fixture("container-host-access")?;
    link("/etc/hostname", &dir.join("conf.ini"))?;
    let found = refused(&home, &dir, &[])?;
    for (at, access) in [
        ("machines.web.privileged", "privileged"),
        ("machines.web.hardware.devices[0]", "devices"),
        ("machines.web.host_folders[0].from", "host-folders"),
        ("machines.web.host_folders[1].from", "docker-socket"),
    ] {
        assert!(has(&found, at, &format!("`--allow-host {access}`")), "{at} in {found:#?}");
    }
    assert!(has(&found, "machines.web.files[0].source", "leads out of the project folder"), "{found:#?}");
    let allowed = [
        "--allow-host",
        "privileged",
        "--allow-host",
        "devices",
        "--allow-host",
        "host-folders",
        "--allow-host",
        "docker-socket",
    ];
    // Allowed, the host's own accesses pass; a link out of the project never does.
    let found = refused(&home, &dir, &allowed)?;
    assert!(!found.iter().any(|l| l.contains("--allow-host")), "{found:#?}");
    assert!(has(&found, "machines.web.files[0].source", "leads out of the project folder"), "{found:#?}");
    // And generate writes nothing from it.
    let mut args: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"generate", &dir];
    for a in &allowed {
        args.push(a);
    }
    let run = isoloom(&home, &args)?;
    assert_ne!(run.code, Some(0), "{run:?}");
    assert!(!dir.join(".isoloom").exists(), "nothing is written");
    Ok(())
}

#[test]
fn a_vm_booting_from_the_hosts_files_is_refused() -> Outcome {
    let home = scratch("host-reach-home")?;
    let dir = fixture("vm-host-boot")?;
    let found = refused(&home, &dir, &[])?;
    for at in ["kernel", "initrd", "network.tftp_root"] {
        let at = format!("machines.vm1.hardware.boot.{at}");
        assert!(has(&found, &at, "`--allow-host boot-files`"), "{at} in {found:#?}");
    }
    assert!(has(&found, "machines.vm1.host_folders[0].from", "`--allow-host host-folders`"), "{found:#?}");
    Ok(())
}

#[test]
fn cloud_settings_for_the_hosts_tools_and_command_lines_as_requirements_are_refused() -> Outcome {
    let home = scratch("host-reach-home")?;
    let dir = fixture("cloud-host-commands")?;
    let found = refused(&home, &dir, &[])?;
    for name in ["LD_PRELOAD", "TF_CLI_ARGS_apply", "TF_CLI_CONFIG_FILE"] {
        assert!(
            has(&found, &format!("cloud.env.{name}"), "changes how the tools on this computer run"),
            "{name} in {found:#?}"
        );
    }
    for i in 0..2 {
        assert!(has(&found, &format!("cloud.requires[{i}]"), "isn't a command's name"), "{i} in {found:#?}");
    }
    // Nothing allows them.
    let all = ["--allow-host", "host-folders", "--allow-host", "privileged", "--allow-host", "devices"];
    assert_eq!(refused(&home, &dir, &all)?.len(), found.len());
    Ok(())
}

#[test]
fn a_value_that_reads_like_syntax_is_carried_as_text() -> Outcome {
    let home = scratch("host-reach-home")?;
    let dir = fixture("quoted-values")?;
    let run = isoloom(&home, &[&"validate", &dir])?;
    assert_eq!(run.code, Some(0), "{run:?}");
    for target in ["vagrant", "proxmox"] {
        let run = isoloom(&home, &[&"generate", &dir, &"--target", &target])?;
        assert_eq!(run.code, Some(0), "{target}: {run:?}");
    }
    let mut stack = vec![dir.join(".isoloom")];
    let mut seen = 0;
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "tf") {
                let text = std::fs::read_to_string(&path)?;
                // Terraform reads `${` and `%{` as its own: the value keeps them doubled.
                let bare = text.match_indices("${file(").any(|(i, _)| !text[..i].ends_with('$'));
                assert!(!bare, "{} evaluates the value", path.display());
                seen += usize::from(text.contains("$${file("));
            }
        }
    }
    assert!(seen > 0, "the value reaches a Terraform file, escaped");
    Ok(())
}

#[test]
#[cfg(unix)]
fn a_build_context_linked_out_of_the_project_is_refused() -> Outcome {
    let home = scratch("host-reach-home")?;
    let dir = fixture("build-context-link")?;
    link("/etc", &dir.join("ctx"))?;
    let found = refused(&home, &dir, &[])?;
    assert!(has(&found, "machines.web.app.build", "leads out of the project folder"), "{found:#?}");
    // The same context inside the project, through a link that stays there, passes.
    std::fs::remove_file(dir.join("ctx"))?;
    link("real", &dir.join("ctx"))?;
    let run = isoloom(&home, &[&"validate", &dir])?;
    assert_eq!(run.code, Some(0), "{run:?}");
    Ok(())
}

#[test]
fn a_path_out_of_the_project_as_written_is_refused_on_every_host() -> Outcome {
    let home = scratch("host-reach-home")?;
    let spec = |build: &str, source: &str| {
        format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  web:\n    networks: {{ lan: 10 }}\n    app: {{ build: '{build}' }}\n    files: [{{ path: /etc/a, source: '{source}' }}]\n"
        )
    };
    for (build, source, why) in [
        ("..\\..\\x", "C:/x", ["it holds a `\\`", "it starts with a drive"]),
        ("../x", "/etc/hostname", ["it goes up a folder", "it starts at the root"]),
        ("a/../../x", "C:x", ["it goes up a folder", "it starts with a drive"]),
    ] {
        let dir = project("host-reach-written", &spec(build, source))?;
        let found = refused(&home, &dir, &[])?;
        assert!(has(&found, "machines.web.app.build", why[0]), "{build}: {found:#?}");
        assert!(has(&found, "machines.web.files[0].source", why[1]), "{source}: {found:#?}");
    }
    Ok(())
}
