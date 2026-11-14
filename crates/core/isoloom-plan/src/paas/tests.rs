use isoloom_spec::model::Target;

use crate::Lab;

const SPEC: &str = "version: 1\nname: shop\ndns: { private: true }\nnetworks: { lab: { cidr: 10.30.0.0/24 } }\nsecrets:\n  db-password: { env: SHOP_DB_PASSWORD }\n  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, app: { image: nginx, steps: [{ packages: [nginx] }] } }\nservices:\n  db:\n    kind: postgres\n    version: \"16\"\n    networks: { lab: 40 }\n    database: shop\n    user: shop\n    password: db-password\n    clients: { web: { env_prefix: DATABASE } }\n  files:\n    kind: object-storage\n    bucket: uploads\n    networks: { lab: 41 }\n    credentials: { access_key: files-key, secret_key: files-secret }\n";

fn lab(yaml: &str) -> Lab {
    let lab = Lab::parse(yaml).unwrap();
    assert!(lab.problems().is_empty(), "{:?}", lab.problems());
    lab
}

#[test]
fn a_container_target_runs_each_service_as_a_machine_its_clients_wait_for() {
    let spec = lab(SPEC).for_target(Target::Docker);
    assert!(spec.services.is_empty(), "{:?}", spec.services);
    let names: Vec<&str> = spec.machines.keys().map(String::as_str).collect();
    assert_eq!(names, ["web", "db", "files"]);
    let web = &spec.machines["web"];
    assert!(web.depends_on.iter().any(|d| d == "db"), "{:?}", web.depends_on);
    let env = &web.environment;
    assert_eq!(env.get("DATABASE_HOST").map(String::as_str), Some("db.shop.internal"));
    assert_eq!(env.get("DATABASE_PORT").map(String::as_str), Some("5432"));
    assert_eq!(env.get("DATABASE_PASSWORD_FILE").map(String::as_str), Some("/run/isoloom/services/db/password"));
    assert_eq!(env.get("DATABASE_SSLMODE").map(String::as_str), Some("prefer"));
    assert!(!env.values().any(|v| v.contains("SHOP_DB_PASSWORD")), "{env:?}");
}

#[test]
fn a_cloud_keeps_its_services_managed_and_its_clients_get_the_same_names() {
    let spec = lab(SPEC).for_target(Target::CloudVm);
    assert_eq!(spec.services.keys().collect::<Vec<_>>(), ["db", "files"]);
    assert!(!spec.machines.contains_key("db"));
    assert_eq!(
        spec.machines["web"].environment.get("DATABASE_HOST").map(String::as_str),
        Some("db.shop.internal")
    );
}

#[test]
fn a_target_that_cant_render_a_service_refuses_it_at_its_field() {
    let l = lab(SPEC);
    let refused = l.refusal(Target::Proxmox).unwrap();
    assert_eq!(refused.path.to_string(), "services.files");
    assert!(refused.reason.contains("pinned VM form"), "{}", refused.reason);
    assert!(l.refusal(Target::Docker).is_none(), "{:?}", l.refusal(Target::Docker));
    let postgres_only = SPEC
        .split("  files:\n")
        .next()
        .unwrap()
        .replace("  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\n", "");
    let pg = lab(&postgres_only);
    assert!(pg.refusal(Target::Proxmox).is_none(), "{:?}", pg.refusal(Target::Proxmox));
    assert!(pg.for_target(Target::Proxmox).machines.contains_key("db"));
}

#[test]
fn an_object_storage_client_is_refused_on_the_clouds_until_they_can_deliver_its_facts() {
    let used = SPEC.replace("credentials: {", "clients: { web: {} }\n    credentials: {");
    let refused = lab(&used).refusal(Target::CloudVm).unwrap();
    assert!(refused.reason.contains("once applied"), "{}", refused.reason);
}
