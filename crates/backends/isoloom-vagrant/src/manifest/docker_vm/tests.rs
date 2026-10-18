use isoloom_manifest::Part;
use proptest::prelude::*;

use super::*;

/// The words `sh` reads from `line` (single quotes and backslash escapes only, as Vagrant's
/// wrapping needs).
fn sh_words(line: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut chars) = (Vec::new(), None::<String>, false, line.chars());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '\'') => quoted = false,
            (false, '\'') => {
                quoted = true;
                word.get_or_insert_with(String::new);
            }
            (false, '\\') => word.get_or_insert_with(String::new).extend(chars.next()),
            (false, ' ') => words.extend(word.take()),
            (_, c) => word.get_or_insert_with(String::new).push(c),
        }
    }
    words.extend(word);
    words
}

/// The command Vagrant hands `bash -l -c`, from the step's last-but-one argument.
fn as_vagrant_runs_it(command: &str) -> String {
    let Ok(Run::Argv(HostArgv::Every(argv))) = vagrant_ssh(&Template::text(command)) else {
        panic!("an argv")
    };
    let escaped: String = argv.args()[3]
        .parts()
        .iter()
        .map(|p| match p {
            Part::Text(t) => t.clone(),
            Part::Placeholder(p) => panic!("{p:?}"),
        })
        .collect();
    // Vagrant: `command = "#{shell} -c '#{command}'"`.
    let wrapped = format!("bash -l -c '{escaped}'");
    sh_words(&wrapped).pop().unwrap()
}

#[test]
fn a_command_runs_on_the_vms_one_machine_without_a_terminal() {
    let Ok(Run::Argv(HostArgv::Every(argv))) = vagrant_ssh(&Template::text("docker ps")) else {
        panic!()
    };
    let words: Vec<String> = argv.args().iter().map(ToString::to_string).collect();
    assert_eq!(words, ["ssh", "--no-tty", "--command", "docker ps", "default"]);
    assert_eq!(as_vagrant_runs_it("cat 'a b'"), "cat 'a b'");
}

#[test]
fn a_placeholder_is_left_for_the_runner() {
    let t = Template::from_parts([
        Part::Text("echo 'x' ".into()),
        Part::Placeholder(isoloom_manifest::Placeholder::Run(isoloom_manifest::RunVar::Name)),
    ]);
    let Ok(Run::Argv(HostArgv::Every(argv))) = vagrant_ssh(&t) else { panic!() };
    assert_eq!(argv.args()[3].to_string(), "echo '\\''x'\\'' ${run.name}");
}

proptest! {
    #[test]
    fn vagrant_hands_bash_the_command_as_written(command in "[ -~]{0,60}") {
        prop_assert_eq!(as_vagrant_runs_it(&command), command);
    }
}
