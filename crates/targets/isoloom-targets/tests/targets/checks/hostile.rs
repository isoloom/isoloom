//! A check's own text (its name, URL and text to find) is data on every target that writes a
//! check runner: quotes, `$(…)`, backticks, line breaks and a line that is a here-document's
//! terminator reach the runner as written, and nothing of them runs. Each target's runner is
//! generated from a spec carrying them, checked by `sh -n`, then run with stand-in tools; a
//! command hidden in the text would leave a marker file, which must never appear. The Vagrantfile
//! hands its runner over through a here-document whose terminator must be no line of it.

use isoloom_spec::Target;
use isoloom_targets::generate;
use isoloom_test_support::FakeBin;

use crate::testing::{Outcome, lab};

/// The spec's check name, URL and text to find, as the check holds them, each carrying a
/// command that would write `marker`.
fn hostile_texts(marker: &str) -> [String; 3] {
    let run = |n: u8| format!("touch {marker}.{n}");
    [
        format!("a'$({})`{}`\nISOLOOM_CHECKS\n{}\nEOF\n", run(1), run(2), run(3)),
        format!("http://web/'$({})", run(4)),
        format!("x'\"$({})`{}`\nISOLOOM_EOF\n{}", run(5), run(6), run(7)),
    ]
}

/// The spec, with every check text carrying a command that would write `marker`.
fn hostile_spec(marker: &str) -> String {
    let run = |n: u8| format!("touch {marker}.{n}");
    let name = format!("a'$({})`{}`\\nISOLOOM_CHECKS\\n{}\\nEOF\\n", run(1), run(2), run(3));
    let url = format!("http://web/'$({})", run(4));
    let text = format!("x'\\\"$({})`{}`\\nISOLOOM_EOF\\n{}", run(5), run(6), run(7));
    format!(
        "version: 1\nname: hostile\nnetworks: {{ lan: {{ cidr: 10.9.0.0/24 }} }}\nmachines:\n  web: {{ os: debian@12, networks: {{ lan: 10 }}, ports: {{ web: {{ port: 80, protocol: http }} }} }}\n  user: {{ os: debian@12, networks: {{ lan: 20 }}, access: true }}\nchecks:\n  - name: \"{name}\"\n    from: user\n    http: \"{url}\"\n    contains: \"{text}\"\n    wait: 0\n"
    )
}

/// The markers a hidden command left.
fn left(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("pwn."))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn hostile_check_text_is_data_in_every_targets_check_runner() -> Outcome {
    let bin = FakeBin::named("hostile-checks")?;
    let marker = bin.dir().join("pwn");
    let spec = lab(&hostile_spec(&marker.to_string_lossy()))?;
    // The tools a runner calls answer, and every text tool it uses is the host's own.
    bin.stand_in("curl", "echo '<html>page</html>'")?
        .stand_in("wget", "echo '<html>page</html>'")?
        .stand_in("sleep", ":")?
        // `timeout <seconds> <command>...` runs the command (macOS has no `timeout` of its own).
        .stand_in("timeout", "shift; exec \"$@\"")?;
    bin.link_real(&[
        "sh", "cat", "grep", "head", "tail", "printf", "date", "tr", "sed", "mktemp", "rm", "touch", "env",
    ])?;
    let mut runners = 0;
    for target in [
        Target::Docker,
        Target::Vagrant,
        Target::CloudVm,
        Target::Proxmox,
        Target::Esxi,
        Target::Kubernetes,
    ] {
        let files = generate(&spec, target)?;
        for runner in files
            .iter()
            .filter(|f| f.path.contains("/checks/") && std::path::Path::new(&f.path).extension().is_some_and(|e| e.eq_ignore_ascii_case("sh")))
        {
            runners += 1;
            let syntax = bin.run(&format!("sh -n {}", isoloom_spec::shell::quote(&write(&bin, &runner.contents)?)), &[])?;
            assert!(
                syntax.status.success(),
                "{target:?} {}: {}",
                runner.path,
                String::from_utf8_lossy(&syntax.stderr)
            );
            let out = bin.run(
                &format!("ISOLOOM_DERIVED=0 sh {} || true", isoloom_spec::shell::quote(&write(&bin, &runner.contents)?)),
                &[],
            )?;
            let said = String::from_utf8_lossy(&out.stdout);
            // The texts reach the runner as written: each the one quoted word of its value.
            for text in hostile_texts(&marker.to_string_lossy()) {
                let word = isoloom_spec::shell::quote(&text);
                assert!(
                    runner.contents.contains(&word),
                    "{target:?} {}: {word} isn't in the runner as written:\n{}",
                    runner.path,
                    runner.contents
                );
            }
            assert!(
                said.contains("isoloom-check: END"),
                "{target:?} {}: the runner ran to its end: {said}{}",
                runner.path,
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(left(bin.dir()), Vec::<String>::new(), "{target:?} {}: the check's text ran", runner.path);
        }
        if target == Target::Vagrant {
            installer_ends_on_no_line_of_its_runner(&files)?;
        }
    }
    assert!(runners >= 6, "every target writes a runner: {runners}");
    Ok(())
}

/// The runner `text` written to a file of `bin`'s folder: its path.
fn write(bin: &FakeBin, text: &str) -> Outcome<String> {
    let path = bin.dir().join("runner.sh");
    std::fs::write(&path, text)?;
    Ok(path.to_string_lossy().into_owned())
}

/// The Vagrantfile's installer of each runner closes its here-document on a line the runner
/// doesn't have, even with a check's line `ISOLOOM_CHECKS`.
fn installer_ends_on_no_line_of_its_runner(files: &[isoloom_targets::GeneratedFile]) -> Outcome {
    let vagrantfile = files.iter().find(|f| f.path.ends_with("/Vagrantfile")).ok_or("no Vagrantfile")?;
    let runner = files.iter().find(|f| f.path.ends_with("vagrant/checks/user.sh")).ok_or("no runner for user")?;
    let tag = vagrantfile
        .contents
        .split("cat > /var/lib/isoloom/checks.sh.new <<'")
        .nth(1)
        .and_then(|rest| rest.split('\'').next())
        .ok_or("no installer here-document")?;
    assert_ne!(tag, "ISOLOOM_CHECKS", "the check's own line would end it");
    assert!(runner.contents.lines().all(|l| l != tag), "{tag} is a line of the runner");
    Ok(())
}
