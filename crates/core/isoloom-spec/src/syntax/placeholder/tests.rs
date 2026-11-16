use proptest::prelude::*;

use super::*;
use crate::Error;

fn parsed(path: &str) -> Reference {
    path.parse().unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn each_path_of_the_grammar_reads_into_its_reference_and_writes_back_as_itself() {
    assert_eq!(parsed("machine.name"), Reference::MachineName);
    assert_eq!(parsed("machine.addresses.lab"), Reference::OwnAddress { network: "lab".into() });
    assert!(matches!(
        parsed("machines.db.ports.postgres.port"),
        Reference::Port { field: PortField::Port, .. }
    ));
    assert!(parsed("machines.web.ports.site.url").is_run_time() && parsed("machines.web.ports.site.published").is_run_time());
    assert_eq!(parsed("os.codename"), Reference::Os(OsField::Codename));
    assert_eq!(
        parsed("with.password.path"),
        Reference::Setting {
            name: "password".into(),
            field: SettingField::Path
        }
    );
    assert_eq!(parsed("item.name"), Reference::Item("name".into()));
    assert_eq!(parsed("item.password.path"), Reference::ItemPath("password".into()));
    for path in [
        "machine.name",
        "version",
        "os.family",
        "os.version",
        "os.codename",
        "with.password",
        "item.name",
        "item.password.path",
        "machines.db.ports.postgres.port",
    ] {
        assert_eq!(parsed(path).to_string(), path);
    }
}

#[test]
fn a_path_outside_the_grammar_says_the_paths_there() {
    for (path, hint) in [
        ("machines.web.ports.0.publish", "ports.<name>.port"),
        ("machine.ip", "machine.name"),
        ("os.arch", "os.codename"),
        ("env.HOME", "paths start with"),
    ] {
        let e = path.parse::<Reference>().unwrap_err().to_string();
        assert!(e.contains(&format!("`{{{{ {path} }}}}`")) && e.contains(hint), "{e}");
    }
}

#[test]
fn each_place_names_only_its_own_paths() {
    let run_time = read("{{ machines.web.ports.site.published }}", Place::Template).unwrap_err();
    assert!(matches!(&run_time[0], Error::NotHere { why, .. } if why.contains("only once the environment runs")));
    // `None`: read there; `Some(true)`: a path of the grammar not read there; `Some(false)`: no
    // path of the grammar.
    for (text, place, refused) in [
        ("listen {{ machine.addresses.lab }}:{{ machines.db.ports.pg.port }}", Place::Template, None),
        ("{{ with.port }}", Place::Template, Some(true)),
        ("{{ with.port }}", Place::Entry, None),
        ("{{ item.name }}", Place::Entry, Some(true)),
        ("{{ item.name }}", Place::EntryItem, None),
        ("{{ machine.name }}", Place::Message, Some(true)),
        ("{{ cloud.outputs.x }}", Place::Template, Some(false)),
    ] {
        match (read(text, place), refused) {
            (Ok(_), None) => {}
            (Err(e), Some(true)) => assert!(matches!(e.as_slice(), [Error::NotHere { .. }]), "{text} in {place:?}: {e:?}"),
            (Err(e), Some(false)) => assert!(matches!(e.as_slice(), [Error::Placeholder { .. }]), "{text} in {place:?}: {e:?}"),
            (got, _) => panic!("{text} in {place:?}: {got:?}"),
        }
    }
}

#[test]
fn a_text_reads_into_its_pieces_in_order() {
    let parts = read("server {{ machine.name }};\n", Place::Template).unwrap();
    assert_eq!(
        parts,
        [
            Part::Text("server ".into()),
            Part::Placeholder(Reference::MachineName),
            Part::Text(";\n".into())
        ]
    );
    let Err(unclosed) = read("a {{ b", Place::Template) else {
        panic!("an unclosed placeholder")
    };
    assert!(matches!(unclosed.as_slice(), [Error::Unclosed]));
    assert_eq!(placeholders("{{ machine.name }} {{ nope }} {{", Place::Template).len(), 3);
}

#[test]
fn join_writes_each_placeholders_value_in_its_place() {
    let parts = read("server {{ machine.name }}:{{ machines.db.ports.pg.port }};", Place::Template).unwrap();
    let filled = join(parts.clone(), |r| {
        Ok::<_, ()>(if *r == Reference::MachineName { "web".to_owned() } else { "5432".to_owned() })
    });
    assert_eq!(filled, Ok("server web:5432;".to_owned()));
    assert_eq!(join(parts, |_| Err("no value")), Err("no value"));
}

proptest! {
    /// A reference written back reads as itself.
    #[test]
    fn a_reference_reads_back_as_itself(
        m in "[a-z][a-z0-9-]{0,8}",
        n in "[a-z][a-z0-9-]{0,8}",
        field in prop::sample::select(vec!["port", "published", "url"]),
    ) {
        for path in [format!("machines.{m}.ports.{n}.{field}"), format!("machines.{m}.addresses.{n}"), format!("machine.addresses.{n}"), format!("with.{n}.path"), format!("vars.{n}")] {
            let r: Reference = path.parse().unwrap();
            prop_assert_eq!(r.to_string(), path);
        }
    }

    /// Any text reads, or says what is wrong with it, never panics.
    #[test]
    fn any_text_reads_or_says_why(text in "\\PC{0,60}") {
        if let Err(wrong) = read(&text, Place::Template) {
            prop_assert!(!wrong.is_empty());
        }
    }
}
