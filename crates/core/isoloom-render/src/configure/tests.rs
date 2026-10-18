//! The configuration renderers' shared parts: a machine's templates, and the files they add.

use isoloom_spec::model::Target;

use super::*;
use crate::testing;

#[test]
fn a_template_is_filled_in_for_its_machine_on_its_target() {
    let env = testing::machine(
        "os: debian@12, app: { steps: [{ template: { from: conf/a.conf, to: /etc/a.conf } }] }",
        Target::Vagrant,
        &[("conf/a.conf", "name={{ machine.name }} at={{ machine.addresses.lan }}\n")],
    );
    let c = Configured::of(&env, "m").expect("a recipe");
    assert_eq!(c.template(&"conf/a.conf".parse().unwrap()).as_deref(), Ok("name=m at=10.9.0.10\n"));
    let unread = c.template(&"conf/b.conf".parse().unwrap()).unwrap_err();
    assert!(unread.contains("wasn't read when the files were generated"), "{unread}");
}

#[test]
fn a_machine_without_steps_has_nothing_to_configure() {
    let env = testing::machine("os: debian@12", Target::Vagrant, &[]);
    assert!(Configured::of(&env, "m").is_none());
    assert_eq!(files(&env), []);
}

#[test]
fn each_library_is_generated_once_when_a_recipe_calls_it() {
    let scripts = testing::machine("os: debian@12, app: { steps: [{ run: a.sh }] }", Target::Vagrant, &[]);
    assert_eq!(files(&scripts), [], "scripts alone call no library");
    let linux = testing::machine("os: debian@12, app: { steps: [{ packages: [curl] }] }", Target::Vagrant, &[]);
    let on_linux: Vec<String> = files(&linux).into_iter().map(|f| f.path).collect();
    assert_eq!(on_linux, [shell::LIBRARY_PATH]);
    let windows = testing::machine("os: windows-server@2022, app: { steps: [{ packages: [Web-Server] }] }", Target::Vagrant, &[]);
    let on_windows: Vec<String> = files(&windows).into_iter().map(|f| f.path).collect();
    assert_eq!(on_windows, [powershell::LIBRARY_PATH]);
}

#[test]
fn a_users_password_hashes_the_same_for_the_same_lab() {
    let env = testing::machine(
        "os: debian@12, app: { steps: [{ user: { name: alice, password: winter } }] }",
        Target::Vagrant,
        &[],
    );
    let c = Configured::of(&env, "m").unwrap();
    assert_eq!(c.password("alice", "winter"), c.password("alice", "winter"));
    assert!(c.password("alice", "winter").starts_with("$6$"));
}
