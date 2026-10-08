//! How the Vagrant outputs copy the project into a VM: Isoloom's own `project` step, a tar.gz
//! built in Ruby when the step runs (not a list of top-level entries read when the Vagrantfile
//! loads), symbolic links kept as links. The Ruby itself runs here when `ruby` and `tar` are on
//! the PATH (CI has both), on a project with a symlink loop.

use std::path::{Path, PathBuf};
use std::process::Command;

use isoloom_core::{Target, generate, load};

fn example(name: &str) -> isoloom_core::Spec {
    load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)).expect("example parses")
}

fn file(spec: &isoloom_core::Spec, target: Target, suffix: &str) -> String {
    generate(spec, target)
        .unwrap()
        .into_iter()
        .find(|f| f.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix}"))
        .contents
}

#[test]
fn every_vagrantfile_copies_the_project_with_its_own_step() {
    let vagrant = file(&example("hello-stack"), Target::Vagrant, "vagrant/Vagrantfile");
    let docker_vm = file(&example("hello-stack"), Target::DockerVm, "docker-vm/Vagrantfile");
    let hybrid = file(&example("mixed-office"), Target::Hybrid, "hybrid/Vagrantfile");
    for (name, v) in [("vagrant", &vagrant), ("docker-vm", &docker_vm), ("hybrid", &hybrid)] {
        assert!(!v.contains("Dir.children(ROOT)"), "{name}: the project is listed when the Vagrantfile loads");
        assert!(!v.contains("provision \"file\""), "{name}: a file provisioner copies the project");
        assert!(v.contains("provisioner(:isoloom_project)"), "{name}: no project provisioner");
    }
    // One step per VM with provisioning (web, cache) and the controller isn't there: hello-stack
    // has no environment-level playbooks. Isoloom's outputs stay out.
    assert!(vagrant.contains("    m.vm.provision \"isoloom_project\", name: \"project\"\n"), "{vagrant}");
    // docker-vm runs the generated Compose file: Isoloom's outputs go in.
    assert!(docker_vm.contains("  config.vm.provision \"isoloom_project\", name: \"project\", generated: true\n"));
    // hybrid's Docker host gets its Compose file, and nothing else of the outputs.
    assert!(hybrid.contains("m.vm.provision \"isoloom_project\", name: \"project\", extra: [\".isoloom/hybrid/compose.yml\"]\n"));
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
}

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("isoloom-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn the_archive_keeps_links_as_links_and_survives_a_loop() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    if !have("ruby") || !have("tar") {
        eprintln!("skipped: needs ruby and tar");
        return;
    }
    let dir = scratch("project-tar");
    let root = dir.join("project");
    for d in ["vendor/goat", "empty", ".git", ".isoloom/hybrid", ".isoloom/vagrant/.vagrant", "sub/.vagrant"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    std::fs::set_permissions(root.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.join(".git/HEAD"), "ref").unwrap();
    std::fs::write(root.join(".isoloom/hybrid/compose.yml"), "services: {}\n").unwrap();
    std::fs::write(root.join(".isoloom/vagrant/Vagrantfile"), "").unwrap();
    std::fs::write(root.join("sub/.vagrant/private_key"), "secret").unwrap();
    // Kubernetes Goat's vendored tree: a link to its own folder, and two links to each other.
    symlink(".", root.join("vendor/goat/self")).unwrap();
    symlink("loop2", root.join("loop1")).unwrap();
    symlink("loop1", root.join("loop2")).unwrap();
    // Longer than a tar header's 100 bytes: a pax header carries it.
    let long = format!("{}/{}.txt", "d".repeat(120), "f".repeat(110));
    std::fs::create_dir_all(root.join(&long).parent().unwrap()).unwrap();
    std::fs::write(root.join(&long), "long").unwrap();

    let rb = dir.join("project.rb");
    std::fs::write(&rb, file(&example("hello-stack"), Target::Vagrant, "vagrant/Vagrantfile")).unwrap();
    let run = |generated: bool, out: &str| {
        let script = format!(
            "eval(File.read(ARGV[0]).split(\"\\nVagrant.configure\").first.sub(/^ROOT = .*$/, \"\")); File.open(ARGV[2], \"wb\") {{ |f| IsoloomProject.write(ARGV[1], f, generated: {generated}, extra: [\".isoloom/hybrid/compose.yml\"]) }}"
        );
        let st = Command::new("ruby")
            .arg("-e")
            .arg(script)
            .arg(&rb)
            .arg(&root)
            .arg(dir.join(out))
            .status()
            .unwrap();
        assert!(st.success(), "the archive wasn't written");
        let out = Command::new("tar").arg("-tvzf").arg(dir.join(out)).output().unwrap();
        assert!(out.status.success(), "tar can't read it: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };

    let list = run(false, "project.tgz");
    let has = |l: &str, entry: &str| l.lines().any(|x| x.ends_with(entry));
    assert!(has(&list, "vendor/goat/self -> ."), "{list}");
    assert!(has(&list, "loop1 -> loop2"), "{list}");
    assert!(has(&list, "empty/"), "{list}");
    assert!(has(&list, &long), "{list}");
    assert!(list.lines().any(|x| x.starts_with("-rwx") && x.ends_with(" run.sh")), "{list}");
    assert!(has(&list, ".isoloom/hybrid/compose.yml"), "{list}");
    for gone in [".git/", "HEAD", "private_key", ".isoloom/vagrant/"] {
        assert!(!list.contains(gone), "{gone} went in: {list}");
    }
    let all = run(true, "all.tgz");
    assert!(has(&all, ".isoloom/vagrant/Vagrantfile"), "{all}");
    assert!(!all.contains(".vagrant/"), "{all}");

    // Unpacked as the VM does.
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let st = Command::new("tar")
        .arg("-xzf")
        .arg(dir.join("project.tgz"))
        .arg("-C")
        .arg(&out)
        .status()
        .unwrap();
    assert!(st.success());
    assert_eq!(std::fs::read_link(out.join("vendor/goat/self")).unwrap(), Path::new("."));
    assert_eq!(std::fs::read_to_string(out.join(&long)).unwrap(), "long");
    let _ = std::fs::remove_dir_all(&dir);
}
