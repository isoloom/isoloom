//! `with:` read against an entry's settings: typed, defaults filled in, each problem coded at
//! its key.

use indexmap::IndexMap;
use isoloom_spec::syntax::{self, Code, FieldPath, Problem};

use super::*;
use crate::entry::Setting;

/// The settings of a made-up entry, written as `entry.yml` writes them.
fn settings(yaml: &str) -> IndexMap<SettingName, Setting> {
    serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"))
}

const SETTINGS: &str = r#"
port: { type: port, default: 80, description: The port. }
root: { type: folder, description: A folder. }
mode: { type: string, values: [fast, safe], default: safe, description: A mode. }
name: { type: string, pattern: "[a-z]+", description: A name. }
workers: { type: integer, minimum: 1, maximum: 8, description: Workers. }
debug: { type: boolean, description: Debug. }
backend: { type: endpoint, description: A backend. }
password: { type: password, owner: postgres, description: A password. }
users:
  type: list
  max: 2
  description: Users.
  items:
    name: { type: string, pattern: "[a-z]+", required: true, description: Its name. }
    password: { type: password, description: Its password. }
database: { type: string, pattern: "[a-z]+", required: true, description: The database. It is made at the first start. }
"#;

/// A lab of `web` (with `with`) and its peers, read.
fn lab(with: &str) -> syntax::Spec {
    let yaml = format!(
        "version: 1\nname: t\nnetworks: {{ lab: {{ cidr: 10.9.0.0/24 }}, other: {{ cidr: 10.8.0.0/24 }} }}\nsecrets: {{ db: {{ env: DB }} }}\ninputs: [FLAG]\nmachines:\n  web: {{ os: debian@12, service: x, with: {with}, networks: {{ lab: 10 }} }}\n  app: {{ os: debian@12, networks: {{ other: 5, lab: 20 }} }}\n  far: {{ os: debian@12, networks: {{ other: 6 }} }}\n"
    );
    syntax::parse(&yaml).unwrap_or_else(|p| panic!("{yaml}: {p:?}"))
}

/// `with` (inline YAML) read against [`SETTINGS`].
fn read_with(with: &str) -> Result<Settings, Vec<Problem>> {
    let spec = lab(with);
    let given = match &spec.machines["web"].kind {
        syntax::Kind::Service(s) => s.with.clone(),
        _ => IndexMap::new(),
    };
    let around = Around {
        spec: &spec,
        machine: "web",
        family: Family::Debian,
    };
    read("x", &settings(SETTINGS), &given, around, &FieldPath::new("machines.web.with"))
}

/// The one problem `with` has: its code, path and reason.
fn problem(with: &str) -> (Code, String, String) {
    let found = read_with(with).unwrap_err();
    assert_eq!(found.len(), 1, "{found:?}");
    (found[0].code, found[0].path.to_string(), found[0].reason.clone())
}

#[test]
fn each_value_is_typed_and_the_defaults_filled_in() {
    let s = read_with("{ database: shop, root: site/, workers: 4, debug: true, backend: 'app:9000', password: { secret: db } }").unwrap();
    assert_eq!(s.get("port"), Some(&SettingValue::Port(NonZeroU16::new(80).unwrap())));
    assert_eq!(s.get("mode"), Some(&SettingValue::Text("safe".into())));
    assert_eq!(
        s.get("root").and_then(|v| v.text().ok()).as_deref(),
        Some("site"),
        "a folder's text has no trailing slash"
    );
    assert_eq!(s.get("workers").and_then(|v| v.json().ok()), Some(serde_json::json!(4)));
    assert_eq!(
        s.get("backend").and_then(|v| v.text().ok()).as_deref(),
        Some("{{ machines.app.addresses.lab }}:9000"),
        "a machine's endpoint is its address on the network the two share, filled in per target"
    );
    let password = s.get("password").unwrap();
    assert_eq!(password.path(), Ok("/run/isoloom/with/password"));
    assert_eq!(password.text(), Err(NotText::Sensitive), "a secret's value is never written");
    assert!(!s.is_set("name"), "an optional setting with no default stays unset");
}

#[test]
fn an_address_endpoint_and_a_literal_password_are_text() {
    let v4 = read_with("{ database: shop, backend: '10.9.0.7:5432', password: hunter2 }").unwrap();
    assert_eq!(v4.get("backend").and_then(|v| v.text().ok()).as_deref(), Some("10.9.0.7:5432"));
    assert_eq!(v4.get("password").and_then(|v| v.text().ok()).as_deref(), Some("hunter2"));
    let v6 = read_with("{ database: shop, backend: '[fd00::7]:5432' }").unwrap();
    assert_eq!(v6.get("backend").and_then(|v| v.text().ok()).as_deref(), Some("[fd00::7]:5432"));
}

#[test]
fn a_key_outside_the_schema_suggests_the_closest_setting() {
    let found = read_with("{ database: shop, prot: 8080 }").unwrap_err();
    assert_eq!((found[0].code, found[0].path.as_str()), (Code::UnknownField, "machines.web.with.prot"));
    assert_eq!(found[0].reason, "x has no setting `prot`");
    assert_eq!(found[0].fix.as_deref(), Some("did you mean `port`?"));
}

#[test]
fn a_required_setting_left_out_is_missing() {
    let (code, path, reason) = problem("{}");
    assert_eq!((code, path.as_str()), (Code::MissingField, "machines.web.with.database"));
    assert_eq!(reason, "x needs `database`: The database");
}

#[test]
fn a_value_of_the_wrong_type_or_out_of_bounds_is_invalid() {
    for (with, path, reason) in [
        ("{ database: shop, port: 70000 }", "port", "`port` is a port number, 1 to 65535"),
        ("{ database: shop, workers: 9 }", "workers", "`workers` is from 1 to 8, not 9"),
        ("{ database: shop, debug: yes-please }", "debug", "`debug` is `true` or `false`"),
        ("{ database: shop, mode: slow }", "mode", "`slow` isn't one of `mode`'s values"),
        ("{ database: Shop }", "database", "`Shop` doesn't match `database`'s pattern `[a-z]+`"),
        ("{ database: shop, root: /srv/site }", "root", "`root` is a folder of the project (`site/`)"),
        (
            "{ database: shop, backend: app }",
            "backend",
            "`app` isn't `<machine>:<port>` or `<address>:<port>`",
        ),
    ] {
        let (code, at, said) = problem(with);
        assert_eq!((code, at), (Code::InvalidValue, format!("machines.web.with.{path}")), "{with}");
        assert!(said.starts_with(reason), "{with}: {said}");
    }
}

#[test]
fn a_secret_or_an_input_goes_only_to_a_password() {
    let not_a_password = problem("{ database: { secret: db } }");
    assert_eq!(
        (not_a_password.0, not_a_password.1.as_str()),
        (Code::InvalidValue, "machines.web.with.database")
    );
    assert!(
        not_a_password.2.contains("only a `password` setting takes a secret or an input"),
        "{not_a_password:?}"
    );
    let secret = problem("{ database: shop, password: { secret: nope } }");
    assert_eq!((secret.0, secret.1.as_str()), (Code::UnknownReference, "machines.web.with.password.secret"));
    let input = problem("{ database: shop, password: { input: NOPE } }");
    assert_eq!((input.0, input.1.as_str()), (Code::UnknownReference, "machines.web.with.password.input"));
    assert!(read_with("{ database: shop, password: { input: FLAG } }").is_ok());
}

#[test]
fn an_endpoint_names_a_machine_sharing_a_network() {
    let far = problem("{ database: shop, backend: 'far:80' }");
    assert_eq!(
        far,
        (
            Code::InvalidValue,
            "machines.web.with.backend".to_owned(),
            "`far` shares no network with `web`".to_owned()
        )
    );
    let typo = problem("{ database: shop, backend: 'ap:80' }");
    assert_eq!((typo.0, typo.2.as_str()), (Code::UnknownReference, "no machine named `ap`"));
}

#[test]
fn a_list_types_each_item_and_keeps_to_its_most() {
    let s = read_with("{ database: shop, users: [{ name: ann, password: { secret: db } }, { name: bob }] }").unwrap();
    let Some(SettingValue::List(items)) = s.get("users") else { panic!() };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].get("password").and_then(|p| p.path().ok()), Some("/run/isoloom/with/users/1/password"));
    assert_eq!(s.get("users").map(SettingValue::text), Some(Err(NotText::List)));
    let unnamed = problem("{ database: shop, users: [{ password: x }] }");
    assert_eq!((unnamed.0, unnamed.1.as_str()), (Code::MissingField, "machines.web.with.users[0].name"));
    let nick = problem("{ database: shop, users: [{ name: a, nick: b }] }");
    assert_eq!((nick.0, nick.1.as_str()), (Code::UnknownField, "machines.web.with.users[0].nick"));
    let three = problem("{ database: shop, users: [{ name: a }, { name: b }, { name: c }] }");
    assert_eq!((three.0, three.2.as_str()), (Code::InvalidValue, "`users` takes at most 2 items, not 3"));
}

#[test]
fn a_windows_value_file_is_under_the_isoloom_folder() {
    let name: SettingName = "password".parse().unwrap();
    assert_eq!(value_file(Family::Windows, &name, None), "C:\\isoloom\\with\\password");
}
