use proptest::prelude::*;

use super::*;
use crate::Error;

fn parsed(path: &str) -> Reference {
    path.parse().unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn each_path_of_the_grammar_reads_into_its_reference() {
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
    assert_eq!(parsed("cloud.outputs.bucket"), Reference::CloudOutput("bucket".into()));
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
    let template = |text: &str| read(text, Place::Template);
    assert!(template("listen {{ machine.addresses.lab }}:{{ machines.db.ports.pg.port }}").is_ok());
    let run_time = template("{{ machines.web.ports.site.published }}").unwrap_err();
    assert!(matches!(&run_time[0], Error::NotHere { why, .. } if why.contains("only once the lab runs")));
    assert!(template("{{ with.port }}").is_err() && read("{{ with.port }}", Place::Entry).is_ok());
    assert!(read("{{ item.name }}", Place::Entry).is_err() && read("{{ item.name }}", Place::EntryItem).is_ok());
    assert!(read("{{ machine.name }}", Place::Message).is_err() && read("{{ cloud.outputs.x }}", Place::Template).is_err());
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
