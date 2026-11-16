use isoloom_spec::model::Target;

use crate::Lab;

/// No `dns:` line: services turn the private zone on themselves.
const SPEC: &str = "version: 1\nname: shop\nnetworks: { lab: { cidr: 10.30.0.0/24 } }\nsecrets:\n  db-password: { env: SHOP_DB_PASSWORD }\n  files-key: { env: SHOP_FILES_KEY }\n  files-secret: { env: SHOP_FILES_SECRET }\n  bus-user: { env: SHOP_BUS_USER }\n  bus-password: { env: SHOP_BUS_PASSWORD }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] } }\nservices:\n  db:\n    kind: postgres\n    version: \"16\"\n    networks: { lab: 40 }\n    database: shop\n    user: shop\n    password: db-password\n    clients: { web: { env_prefix: DATABASE } }\n  files:\n    kind: object-storage\n    bucket: uploads\n    networks: { lab: 41 }\n    credentials: { access_key: files-key, secret_key: files-secret }\n    clients: { web: { env_prefix: STORAGE } }\n  bus:\n    kind: kafka\n    networks: { lab: 42 }\n    topics: { orders: { partitions: 3 } }\n    credentials: { user: bus-user, password: bus-password }\n    clients: { web: {} }\n";

fn lab(yaml: &str) -> Lab {
    let lab = Lab::parse(yaml).unwrap();
    assert_eq!(lab.problems(), []);
    lab
}

fn env<'a>(spec: &'a isoloom_spec::model::Spec, key: &str) -> Option<&'a str> {
    spec.machines["web"].environment.get(key).map(String::as_str)
}

#[test]
fn services_turn_the_private_zone_on() {
    let spec = lab(SPEC).for_target(Target::Docker);
    assert_eq!(spec.private_domain().as_deref(), Some("shop.internal"));
}

#[test]
fn a_container_target_runs_each_service_as_a_machine_its_clients_wait_for() {
    let spec = lab(SPEC).for_target(Target::Docker);
    assert!(spec.services.is_empty(), "{:?}", spec.services);
    assert_eq!(spec.machines.keys().map(String::as_str).collect::<Vec<_>>(), ["web", "db", "files", "bus"]);
    let web = &spec.machines["web"];
    assert_eq!(web.depends_on, ["db", "files", "bus"]);
    assert_eq!(env(&spec, "DATABASE_HOST"), Some("db.shop.internal"));
    assert_eq!(env(&spec, "DATABASE_PASSWORD_FILE"), Some("/etc/isoloom/services/db/password"));
    assert_eq!(env(&spec, "STORAGE_BUCKET_FILE"), Some("/etc/isoloom/services/files/bucket"));
    assert_eq!(env(&spec, "STORAGE_AUTH"), Some("keys"));
    assert_eq!(env(&spec, "BUS_BOOTSTRAP_FILE"), Some("/etc/isoloom/services/bus/bootstrap"));
    let files: Vec<String> = web.secrets.iter().map(isoloom_spec::model::MachineSecret::path).collect();
    assert_eq!(
        files,
        [
            "/etc/isoloom/services/db/password",
            "/etc/isoloom/services/files/access-key",
            "/etc/isoloom/services/files/secret-key",
            "/etc/isoloom/services/bus/user",
            "/etc/isoloom/services/bus/password"
        ]
    );
}

#[test]
fn a_cloud_keeps_its_services_and_leaves_the_files_it_issues_to_its_delivery() {
    let spec = lab(SPEC).for_target(Target::CloudVm);
    assert_eq!(spec.services.keys().collect::<Vec<_>>(), ["db", "files", "bus"]);
    assert!(!spec.machines.contains_key("db"));
    assert_eq!(env(&spec, "DATABASE_HOST"), Some("db.shop.internal"));
    assert_eq!(env(&spec, "STORAGE_BUCKET_FILE"), Some("/etc/isoloom/services/files/bucket"));
    let files: Vec<String> = spec.machines["web"].secrets.iter().map(isoloom_spec::model::MachineSecret::path).collect();
    assert_eq!(files, ["/etc/isoloom/services/db/password"], "only the database's password is the spec's own");
}

#[test]
fn every_service_runs_as_a_vm_on_a_server() {
    let spec = lab(SPEC).for_target(Target::Proxmox);
    assert_eq!(spec.machines.keys().map(String::as_str).collect::<Vec<_>>(), ["web", "db", "files", "bus"]);
    assert_eq!(lab(SPEC).refusal(Target::Proxmox), None);
}

#[test]
fn an_identity_is_refused_off_the_clouds() {
    let identity = SPEC.replace(
        "clients: { web: { env_prefix: STORAGE } }",
        "clients: { web: { env_prefix: STORAGE, access: identity } }",
    );
    let l = lab(&identity);
    let refused = l.refusal(Target::Docker).unwrap();
    assert_eq!(refused.path.to_string(), "services.files");
    assert_eq!(l.refusal(Target::CloudVm), None);
    assert_eq!(env(&l.for_target(Target::CloudVm), "STORAGE_AUTH"), Some("identity"));
}

#[test]
fn a_spec_of_services_alone_runs_and_publishes_them() {
    let alone = "version: 1\nname: data\nnetworks: { lab: { cidr: 10.31.0.0/24 } }\ninputs:\n  PLAYER_IP: { sensitive: false }\nsecrets:\n  db-password: { env: DATA_DB_PASSWORD }\nservices:\n  db: { kind: postgres, version: \"16\", networks: { lab: 40 }, password: db-password, publish: { allow: PLAYER_IP } }\n";
    let l = lab(alone);
    let possible: Vec<&str> = l.possible().into_iter().map(Target::id).collect();
    assert_eq!(
        possible,
        [
            "docker",
            "hosted",
            "cloud-docker",
            "kubernetes",
            "vagrant",
            "proxmox",
            "esxi",
            "cloud-vm",
            "external"
        ]
    );
    let spec = l.for_target(Target::Docker);
    let ports: Vec<String> = spec.machines["db"]
        .ports
        .iter()
        .map(|p| format!("{} published {:?}", p.port, p.publish))
        .collect();
    assert_eq!(ports, ["5432 published Some(5432)"]);
}

#[test]
#[expect(clippy::disallowed_methods, reason = "the test checks the start scripts with sh -n: test code, not an adapter")]
fn the_start_scripts_are_shell_sh_reads() {
    let spec = isoloom_spec::syntax::parse(SPEC).unwrap();
    let isoloom_spec::paas::Service::Kafka(k) = &spec.services["bus"] else {
        panic!()
    };
    for script in [super::seaweedfs_start("files"), super::kafka_start("bus", "bus.shop.internal", k)] {
        let out = std::process::Command::new("sh").args(["-n", "-c", &script]).output().unwrap();
        assert!(out.status.success(), "{}\n{script}", String::from_utf8_lossy(&out.stderr));
    }
}
