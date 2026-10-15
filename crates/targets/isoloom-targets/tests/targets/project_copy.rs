//! How the outputs copy the project into the machines. Vagrant: Isoloom's own `project` step, a
//! tar.gz built in Ruby when the step runs (not a list of top-level entries read when the
//! Vagrantfile loads), symbolic links kept as links. The Ruby itself runs here when `ruby` and
//! `tar` are on the PATH (CI has both), on a project with a symlink loop. Every output leaves
//! out what `.isoloomignore` lists.

use std::path::{Path, PathBuf};
use std::process::Command;

use isoloom_render::project::ignore::{self, Rules};
use isoloom_spec::Target;

use crate::testing::{Outcome, example, generated as file};

#[test]
fn every_vagrantfile_copies_the_project_with_its_own_step() -> Outcome {
    let vagrant = file(&example("hello-stack")?, Target::Vagrant, "vagrant/Vagrantfile")?;
    let docker_vm = file(&example("hello-stack")?, Target::DockerVm, "docker-vm/Vagrantfile")?;
    let hybrid = file(&example("mixed-office")?, Target::Hybrid, "hybrid/Vagrantfile")?;
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
    Ok(())
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Vagrant runs a Vagrantfile on its own embedded Ruby (3.x). The `ruby` standing in for it here
/// can be older: macOS ships 2.6, from before `Enumerable#filter_map` (2.7), so the script gets
/// it first when it's missing.
const FILTER_MAP_FOR_OLD_RUBY: &str = "Enumerable.module_eval { def filter_map; each_with_object([]) { |x, out| v = yield(x); out << v if v }; end } unless Enumerable.method_defined?(:filter_map); ";

/// Archives `root` with the Ruby of a generated Vagrantfile and lists the archive (`tar -tv`).
fn archive(dir: &Path, root: &Path, generated: bool, out: &str) -> Outcome<String> {
    let rb = dir.join("project.rb");
    std::fs::write(&rb, file(&example("hello-stack")?, Target::Vagrant, "vagrant/Vagrantfile")?)?;
    let script = format!(
        "{FILTER_MAP_FOR_OLD_RUBY}eval(File.read(ARGV[0], encoding: \"UTF-8\").split(\"\\nVagrant.configure\").first.sub(/^ROOT = .*$/, \"\")); File.open(ARGV[2], \"wb\") {{ |f| IsoloomProject.write(ARGV[1], f, generated: {generated}, extra: [\".isoloom/hybrid/compose.yml\"]) }}"
    );
    let st = Command::new("ruby").arg("-e").arg(script).arg(&rb).arg(root).arg(dir.join(out)).status()?;
    if !st.success() {
        return Err("the archive wasn't written".into());
    }
    let list = Command::new("tar").arg("-tvzf").arg(dir.join(out)).output()?;
    if !list.status.success() {
        return Err(format!("tar can't read it: {}", String::from_utf8_lossy(&list.stderr)).into());
    }
    // Lossy: Windows' bsdtar lists names in the console's code page, not UTF-8.
    Ok(String::from_utf8_lossy(&list.stdout).into_owned())
}

/// A fresh folder for one test.
fn scratch(name: &str) -> Outcome<PathBuf> {
    let dir = std::env::temp_dir().join(format!("isoloom-{name}-{}", std::process::id()));
    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => std::fs::create_dir_all(&dir)?,
    }
    Ok(dir)
}

/// A project under `dir` with what the archive must keep (links, a loop of links, an empty
/// folder, an executable, a long path, generated files) and leave out (`.git`, Vagrant's
/// state); its root, and the long path.
#[cfg(unix)]
fn tricky_project(dir: &Path) -> Outcome<(PathBuf, String)> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = dir.join("project");
    for d in ["vendor/goat", "empty", ".git", ".isoloom/hybrid", ".isoloom/vagrant/.vagrant", "sub/.vagrant"] {
        std::fs::create_dir_all(root.join(d))?;
    }
    std::fs::write(root.join("run.sh"), "#!/bin/sh\necho hi\n")?;
    std::fs::set_permissions(root.join("run.sh"), std::fs::Permissions::from_mode(0o755))?;
    std::fs::write(root.join(".git/HEAD"), "ref")?;
    std::fs::write(root.join(".isoloom/hybrid/compose.yml"), "services: {}\n")?;
    std::fs::write(root.join(".isoloom/vagrant/Vagrantfile"), "")?;
    std::fs::write(root.join("sub/.vagrant/private_key"), "secret")?;
    // Kubernetes Goat's vendored tree: a link to its own folder, and two links to each other.
    symlink(".", root.join("vendor/goat/self"))?;
    symlink("loop2", root.join("loop1"))?;
    symlink("loop1", root.join("loop2"))?;
    // Longer than a tar header's 100 bytes: a pax header carries it.
    let long = format!("{}/{}.txt", "d".repeat(120), "f".repeat(110));
    std::fs::create_dir_all(root.join("d".repeat(120)))?;
    std::fs::write(root.join(&long), "long")?;
    Ok((root, long))
}

#[cfg(unix)]
#[test]
fn the_archive_keeps_links_as_links_and_survives_a_loop() -> Outcome {
    if !have("ruby") || !have("tar") {
        eprintln!("skipped: needs ruby and tar");
        return Ok(());
    }
    let dir = scratch("project-tar")?;
    let (root, long) = tricky_project(&dir)?;
    let run = |generated: bool, out: &str| archive(&dir, &root, generated, out);

    let list = run(false, "project.tgz")?;
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
    let all = run(true, "all.tgz")?;
    assert!(has(&all, ".isoloom/vagrant/Vagrantfile"), "{all}");
    assert!(!all.contains(".vagrant/"), "{all}");
    unpacks_as_packed(&dir, &long)?;
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// `dir`'s project.tgz, unpacked as the VM does: the links and the long path as they were.
#[cfg(unix)]
fn unpacks_as_packed(dir: &Path, long: &str) -> Outcome {
    let out = dir.join("out");
    std::fs::create_dir_all(&out)?;
    let st = Command::new("tar").arg("-xzf").arg(dir.join("project.tgz")).arg("-C").arg(&out).status()?;
    assert!(st.success());
    assert_eq!(std::fs::read_link(out.join("vendor/goat/self"))?, Path::new("."));
    assert_eq!(std::fs::read_to_string(out.join(long))?, "long");
    Ok(())
}

#[test]
fn isoloomignore_leaves_paths_out() -> Outcome {
    if !have("ruby") || !have("tar") {
        eprintln!("skipped: needs ruby and tar");
        return Ok(());
    }
    let dir = scratch("isoloomignore")?;
    let root = dir.join("project");
    let files = [
        "a.txt",
        "logs/x.log",
        "logs/keep.txt",
        "src/app.py",
        "src/deeper/z.py",
        "src/build/out.o",
        "build/top.o",
        "docs/sub/deep.md",
        "x.tmp",
        "sub/x.tmp",
        ".env",
        "sub/.env",
        "vendor/big/file.bin",
        "vendor/small.txt",
        "data.json",
        "datazjson",
        "w/a+b.txt",
    ];
    for f in files {
        std::fs::create_dir_all(root.join(f).parent().unwrap()).unwrap();
        std::fs::write(root.join(f), f).unwrap();
    }
    std::fs::write(
        root.join(".isoloomignore"),
        "# not copied into the VMs\n\nlogs/\n!logs/keep.txt\nbuild/\n/x.tmp\nsub/*.tmp\nvendor/big\n.env\ndocs/**\nsrc/*.py\ndata.js?n\nw/a+b.txt   \r\n",
    )
    .unwrap();
    let list = archive(&dir, &root, false, "project.tgz")?;
    let went: Vec<&str> = files.iter().copied().filter(|f| list.lines().any(|l| l.ends_with(&format!(" {f}")))).collect();
    // `src/*.py` is anchored (a `/` in the middle): `src/deeper/z.py` stays; `.env` matches at any
    // depth; `!logs/keep.txt` brings back a file of an ignored folder.
    assert_eq!(went, ["a.txt", "logs/keep.txt", "src/deeper/z.py", "vendor/small.txt", "datazjson"], "{list}");
    assert!(!list.contains("build/"), "an ignored folder's entry went in: {list}");
    // Isoloom's own matcher (the runner's copies) agrees.
    let rules = Rules::parse(&std::fs::read_to_string(root.join(ignore::FILE))?);
    let mut entries = Vec::new();
    walk(&root, "", &rules, &mut entries)?;
    let own: Vec<&str> = files.iter().copied().filter(|f| entries.iter().any(|e| e == f)).collect();
    assert_eq!(own, went);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn terraform_outputs_read_isoloomignore() -> Outcome {
    let spec = example("hello-stack")?;
    // Proxmox writes the files that go through cloud-init.
    let proxmox = file(&spec, Target::Proxmox, "proxmox/main.tf")?;
    assert!(proxmox.contains("project_files = [for f in local.project_paths : {"), "{proxmox}");
    assert!(proxmox.contains("file(\"${local.root}/.isoloomignore\")"), "{proxmox}");
    // The cloud modules' tar walks the project itself and leaves out the ignored files, from the
    // archive's top folder (stripped when unpacked).
    for (target, path) in [
        (Target::CloudVm, "cloud-vm/aws/main.tf"),
        (Target::CloudVm, "cloud-vm/gcp/main.tf"),
        (Target::CloudDocker, "cloud-docker/aws/main.tf"),
        (Target::CloudDocker, "cloud-docker/azure/main.tf"),
        (Target::DockerVm, "docker-vm/proxmox/main.tf"),
    ] {
        let tf = file(&spec, target, path)?;
        assert!(tf.contains("resource \"local_file\" \"isoloom_project\""), "{path}");
        assert!(
            tf.contains(r#"-X \"${abspath(local_file.isoloom_project.filename)}\" -C \"${dirname(local.root)}\" \"${basename(local.root)}\""#),
            "{path}"
        );
        assert!(!tf.contains("-C /opt/isoloom &&"), "{path}: unpacked without stripping the top folder");
        assert!(tf.contains("--strip-components=1"), "{path}");
    }
    Ok(())
}

#[test]
fn windows_machines_get_the_project_before_their_steps() -> Outcome {
    let spec = example("windows-hello")?;
    for path in ["cloud-vm/aws/main.tf", "cloud-vm/azure/main.tf"] {
        let tf = file(&spec, Target::CloudVm, path)?;
        // The host's archive (as a Linux machine's), uploaded once, unpacked by the set-up
        // script (no longer in C:\isoloom, which the project replaces) before the steps.
        assert!(tf.contains("destination = \"C:/Windows/Temp/isoloom-project.tgz\""), "{path}");
        assert!(!tf.contains("destination = \"C:/isoloom/"), "{path}: a step uploaded on its own");
        assert!(tf.contains("-File C:/ProgramData/isoloom/setup.ps1"), "{path}");
        let unpack = tf.find("$dest = 'C:\\\\isoloom'; $strip = 1").expect("the project unpacked");
        // The plan's `Run` step, from C:\\isoloom.
        let step = tf.find("-File 'provision/iis.ps1'").expect("the step");
        assert!(unpack < step, "{path}");
    }
    Ok(())
}

/// The Windows extractor (`untar.ps1`), run by PowerShell where it's installed (GitHub's runners
/// have `pwsh`): the Vagrantfile Ruby's archive, and a host tar's with a top folder to strip.
#[test]
fn the_windows_extractor_unpacks_the_archive() -> Outcome {
    if !have("ruby") || !have("tar") || !have("pwsh") {
        eprintln!("skipped: needs ruby, tar and pwsh");
        return Ok(());
    }
    let dir = scratch("untar")?;
    let root = dir.join("project");
    let long = format!("{}/{}.txt", "d".repeat(120), "f".repeat(110));
    for (f, text) in [
        ("a.txt", "a"),
        ("sub/b c.txt", "b"),
        ("ünï.txt", "u"),
        (long.as_str(), "long"),
        ("skip.log", "x"),
    ] {
        std::fs::create_dir_all(root.join(f).parent().unwrap()).unwrap();
        std::fs::write(root.join(f), text).unwrap();
    }
    std::fs::create_dir_all(root.join("empty")).unwrap();
    std::fs::write(root.join(".isoloomignore"), "*.log\n").unwrap();
    archive(&dir, &root, false, "ruby.tgz")?;
    // A host's tar, as the cloud modules run it (GNU tar's long names, here).
    let st = Command::new("tar")
        .arg("-czf")
        .arg(dir.join("host.tgz"))
        .arg("-C")
        .arg(&dir)
        .arg("project")
        .status()
        .unwrap();
    assert!(st.success());
    let ps1 = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core/isoloom-render/src/untar.ps1");
    for (name, strip) in [("ruby", 0), ("host", 1)] {
        let dest = dir.join(format!("{name}-out"));
        let script = format!(
            "$archive = '{}'; $dest = '{}'; $strip = {strip}; . '{}'",
            dir.join(format!("{name}.tgz")).display(),
            dest.display(),
            ps1.display()
        );
        let out = Command::new("pwsh").args(["-NoProfile", "-Command", &script]).output().unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(std::fs::read_to_string(dest.join("sub/b c.txt")).unwrap(), "b", "{name}");
        assert_eq!(std::fs::read_to_string(dest.join("ünï.txt")).unwrap(), "u", "{name}");
        assert_eq!(std::fs::read_to_string(dest.join(&long)).unwrap(), "long", "{name}");
        assert!(dest.join("empty").is_dir(), "{name}");
        assert_eq!(dest.join("skip.log").exists(), name == "host", "{name}: .isoloomignore");
        assert!(!dir.join(format!("{name}.tgz")).exists(), "{name}: the archive stays");
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// The project's entries a copy takes, as the runner walks it with Isoloom's matcher: parents
/// first, version control and the tools' state left out at any depth, and what the rules leave
/// out (a folder's contents still walked when a `!` may bring one back).
fn walk(root: &Path, dir: &str, rules: &Rules, out: &mut Vec<String>) -> Outcome {
    let mut names: Vec<String> = std::fs::read_dir(root.join(dir))?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    names.sort();
    for name in names.into_iter().filter(|n| ![".git", ".vagrant", ".terraform"].contains(&n.as_str())) {
        let rel = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        if std::fs::symlink_metadata(root.join(&rel))?.is_dir() {
            let folder = format!("{rel}/");
            let ignored = rules.ignored(&folder);
            if !ignored {
                out.push(folder);
            }
            if !ignored || rules.brings_back() {
                walk(root, &rel, rules, out)?;
            }
        } else if !rules.ignored(&rel) {
            out.push(rel);
        }
    }
    Ok(())
}
