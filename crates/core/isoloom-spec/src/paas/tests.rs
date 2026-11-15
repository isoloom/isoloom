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
fn a_bucket_name_a_topic_and_a_published_allow_list_are_checked() {
    let bad = problems(
        "services:\n  files: { kind: object-storage, bucket: Up_Loads, networks: { lab: 41 }, credentials: { access_key: files-key, secret_key: files-secret } }\n",
    );
    assert!(bad.iter().any(|(_, p, _)| p == "services.files.bucket"), "{bad:?}");
    let kafka = problems(
        "services:\n  bus: { kind: kafka, networks: { lab: 42 }, topics: { \"or ders\": { partitions: 0 } }, credentials: { user: files-key, password: files-secret }, publish: { allow: NOPE } }\n",
    );
    let has = |path: &str| kafka.iter().any(|(_, p, _)| p == path);
    assert!(has("services.bus.topics.or ders") && has("services.bus.topics.or ders.partitions"), "{kafka:?}");
    assert!(
        kafka.iter().any(|(c, p, _)| *c == Code::UnknownReference && p == "services.bus.publish.allow"),
        "{kafka:?}"
    );
    let identity = problems(&SERVICES.replace("clients: { web: { env_prefix: DATABASE } }", "clients: { web: { access: identity } }"));
    assert!(identity.iter().any(|(_, p, _)| p == "services.db.clients.web.access"), "{identity:?}");
}

#[test]
fn a_spec_with_services_needs_no_dns_line() {
    let spec = syntax::parse(&format!("{}{SERVICES}", HEAD.replace("dns: { private: true }\n", ""))).unwrap();
    assert!(syntax::check(&spec).is_empty(), "{:?}", syntax::check(&spec));
}

#[test]
fn each_target_places_a_service_or_says_why_not() {
    let spec = parsed(SERVICES).unwrap();
    let (db, files) = (&spec.services["db"], &spec.services["files"]);
    assert_eq!(placement(db, Target::CloudVm), Placement::Managed);
    assert_eq!(placement(files, Target::CloudVm), Placement::Managed);
    assert_eq!(placement(db, Target::Docker), Placement::Machine);
    assert_eq!(placement(files, Target::Kubernetes), Placement::Machine);
    assert_eq!(placement(files, Target::Proxmox), Placement::Machine);
    assert!(matches!(placement(db, Target::External), Placement::Refused(_)));
    let identity = parsed(&SERVICES.replace("credentials: {", "clients: { web: { access: identity } }\n    credentials: {")).unwrap();
    assert_eq!(placement(&identity.services["files"], Target::CloudVm), Placement::Managed);
    assert!(matches!(placement(&identity.services["files"], Target::Docker), Placement::Refused(why) if why.contains("only on a cloud")));
    let bus =
        parsed("services:\n  bus: { kind: kafka, networks: { lab: 42 }, credentials: { user: files-key, password: files-secret }, publish: { allow: X } }\n")
            .unwrap();
    assert!(matches!(placement(&bus.services["bus"], Target::CloudVm), Placement::Refused(_)));
    assert_eq!(placement(&bus.services["bus"], Target::Proxmox), Placement::Machine);
}

#[test]
fn the_schema_has_the_services_and_their_kinds() {
    let schema = serde_json::to_string(&schemars::schema_for!(syntax::Spec)).unwrap();
    for word in ["\"services\"", "\"postgres\"", "\"object-storage\"", "\"env_prefix\""] {
        assert!(schema.contains(word), "{word} missing from the schema");
    }
}

/// The `bus` Kafka service with `topics`.
fn bus(topics: &str) -> Service {
    let spec = parsed(&format!(
        "services:\n  bus: {{ kind: kafka, networks: {{ lab: 42 }}, credentials: {{ user: files-key, password: files-secret }}, topics: {topics} }}\n"
    ))
    .unwrap();
    spec.services["bus"].clone()
}

/// The clouds refusing `service`, each with its problem's path, code and fix.
fn refused_by(service: &Service) -> Vec<(&'static str, String, Code, bool)> {
    cloud_limits("bus", service)
        .into_iter()
        .map(|l| (l.cloud, l.problem.path.to_string(), l.problem.code, l.problem.fix.is_some()))
        .collect()
}

#[test]
fn a_compacted_topic_is_refused_on_event_hubs_standard_and_msk() {
    let found = refused_by(&bus("{ ledger: { cleanup: compact } }"));
    assert!(
        found.contains(&("azure", "services.bus.topics.ledger.cleanup".to_owned(), Code::Refused, true)),
        "{found:?}"
    );
    assert!(
        found.contains(&("aws", "services.bus.topics.ledger.cleanup".to_owned(), Code::Refused, true)),
        "{found:?}"
    );
    assert!(!found.iter().any(|(c, ..)| *c == "gcp"), "Managed Kafka compacts");
}

#[test]
fn more_than_32_partitions_are_refused_on_event_hubs_standard() {
    let found = refused_by(&bus("{ orders: { partitions: 33 } }"));
    assert_eq!(found, [("azure", "services.bus.topics.orders.partitions".to_owned(), Code::Refused, true)]);
    assert!(refused_by(&bus("{ orders: { partitions: 32 } }")).is_empty());
}

#[test]
fn topics_of_several_partition_counts_are_refused_on_msk() {
    let found = refused_by(&bus("{ orders: { partitions: 3 }, audit: { partitions: 1 } }"));
    assert_eq!(found, [("aws", "services.bus.topics".to_owned(), Code::Refused, true)]);
    assert!(refused_by(&bus("{ orders: { partitions: 3 }, audit: { partitions: 3 } }")).is_empty());
}
