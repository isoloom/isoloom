use super::*;
use crate::model::Target;
use crate::syntax::{self, Code, Problems};

const HEAD: &str = "version: 1\nname: shop\ndns: { private: true }\nnetworks: { lab: { cidr: 10.30.0.0/24 } }\nsecrets:\n  db-password: { env: SHOP_DB_PASSWORD }\n  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, app: { image: nginx } }\n";

const SERVICES: &str = "services:\n  db:\n    kind: postgres\n    version: \"16\"\n    networks: { lab: 40 }\n    database: shop\n    user: shop\n    password: db-password\n    clients: { web: { env_prefix: DATABASE } }\n  files:\n    kind: object-storage\n    bucket: uploads\n    networks: { lab: 41 }\n    credentials: { access_key: files-key, secret_key: files-secret }\n";

fn parsed(extra: &str) -> Result<syntax::Spec, Problems> {
    syntax::parse(&format!("{HEAD}{extra}"))
}

fn problems(extra: &str) -> Vec<(Code, String, String)> {
    let spec = parsed(extra).unwrap();
    syntax::check(&spec).into_iter().map(|p| (p.code, p.path.to_string(), p.reason)).collect()
}

#[test]
fn services_parse_by_kind_with_their_defaults() {
    let spec = parsed(SERVICES).unwrap();
    let Service::Postgres(db) = &spec.services["db"] else {
        panic!("{:?}", spec.services["db"])
    };
    assert_eq!((db.database.as_str(), db.user.as_str(), db.size), ("shop", "shop", Size::Small));
    assert_eq!(db.clients["web"].prefix("db"), "DATABASE");
    let Service::ObjectStorage(files) = &spec.services["files"] else { panic!() };
    assert_eq!((files.api, files.versioning, files.bucket.as_str()), (Api::S3, false, "uploads"));
    assert_eq!(Client::default().prefix("my-files"), "MY_FILES");
    assert!(syntax::check(&spec).is_empty(), "{:?}", syntax::check(&spec));
}

#[test]
fn an_unknown_kind_or_field_is_a_typed_problem() {
    let kind = parsed("services:\n  q: { kind: queue, networks: { lab: 5 } }\n").unwrap_err();
    assert!(
        kind.iter().any(|p| p.path.to_string().starts_with("services.q") && p.reason.contains("queue")),
        "{kind:?}"
    );
    let field = parsed("services:\n  db: { kind: postgres, version: \"16\", networks: { lab: 5 }, password: db-password, engine: aurora }\n").unwrap_err();
    assert!(field.iter().any(|p| p.code == Code::UnknownField && p.reason.contains("engine")), "{field:?}");
}

#[test]
fn references_names_and_values_are_checked() {
    let found = problems(
        "services:\n  web:\n    kind: postgres\n    version: \"9\"\n    networks: { dmz: 40 }\n    user: admin\n    password: nope\n    clients: { api: { env_prefix: db } }\n",
    );
    let has = |code: Code, path: &str| found.iter().any(|(c, p, _)| *c == code && p == path);
    assert!(has(Code::Conflict, "services.web"), "{found:?}");
    assert!(has(Code::InvalidValue, "services.web.version"), "{found:?}");
    assert!(has(Code::UnknownReference, "services.web.networks.dmz"), "{found:?}");
    assert!(has(Code::InvalidValue, "services.web.user"), "{found:?}");
    assert!(has(Code::UnknownReference, "services.web.password"), "{found:?}");
    assert!(has(Code::UnknownReference, "services.web.clients.api"), "{found:?}");
    assert!(has(Code::InvalidValue, "services.web.clients.api.env_prefix"), "{found:?}");
}

#[test]
fn a_service_needs_the_private_zone_and_a_valid_bucket_name() {
    let spec = syntax::parse(&format!("{}{SERVICES}", HEAD.replace("dns: { private: true }\n", ""))).unwrap();
    let found = syntax::check(&spec);
    assert!(
        found.iter().any(|p| p.code == Code::MissingField && p.path.to_string() == "dns.private"),
        "{found:?}"
    );
    let bad = problems(
        "services:\n  files: { kind: object-storage, bucket: Up_Loads, networks: { lab: 41 }, credentials: { access_key: files-key, secret_key: files-secret } }\n",
    );
    assert!(bad.iter().any(|(_, p, _)| p == "services.files.bucket"), "{bad:?}");
}

#[test]
fn each_target_places_a_service_or_says_why_not() {
    let spec = parsed(SERVICES).unwrap();
    let (db, files) = (&spec.services["db"], &spec.services["files"]);
    assert_eq!(placement(db, Target::CloudVm), Placement::Managed);
    assert_eq!(placement(files, Target::CloudVm), Placement::Managed);
    assert_eq!(placement(db, Target::Docker), Placement::Machine);
    assert_eq!(placement(files, Target::Kubernetes), Placement::Machine);
    assert_eq!(placement(db, Target::Proxmox), Placement::Machine);
    assert!(matches!(placement(files, Target::Proxmox), Placement::Refused(why) if why.contains("VM form")));
    assert!(matches!(placement(db, Target::External), Placement::Refused(_)));
    let used = parsed(&SERVICES.replace("credentials: {", "clients: { web: {} }\n    credentials: {")).unwrap();
    assert!(matches!(placement(&used.services["files"], Target::CloudVm), Placement::Refused(why) if why.contains("once applied")));
}

#[test]
fn the_schema_has_the_services_and_their_kinds() {
    let schema = serde_json::to_string(&schemars::schema_for!(syntax::Spec)).unwrap();
    for word in ["\"services\"", "\"postgres\"", "\"object-storage\"", "\"env_prefix\""] {
        assert!(schema.contains(word), "{word} missing from the schema");
    }
}
