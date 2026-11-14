//! The platform level lowered (`services:`, the `paas` feature): before a scope lowers the spec,
//! a spec with services has its private zone on (`dns.private`, whatever it says: a service is
//! reached by its name there); each service a target runs as a machine becomes one (the
//! catalogue's `postgres`, `SeaweedFS`'s S3 server, Apache Kafka, each a container or a Debian
//! VM); each client machine gets the service's facts; and a target that can't render a service
//! refuses it at its field. What a cloud renders as its managed service stays in `services`, for
//! its Terraform backend.
//!
//! The clients' contract is the same on every target. What every platform knows when Isoloom
//! generates (a database's name in the zone, its port, its login) is an environment value. What
//! differs per platform or is only known once a cloud made it (a bucket's endpoint and name, the
//! brokers, keys a cloud issues) is a file under [`SERVICES_DIR`], each named by a `<PREFIX>_<FACT>_FILE`
//! variable: written by the machine's set-up where the service is a machine, delivered by
//! Terraform once applied on a cloud.

use std::borrow::Cow;

use indexmap::IndexMap;
use isoloom_spec::model::EnvironmentDns;
use isoloom_spec::paas::{Access, Client, Kafka, ObjectStorage, Placement, Postgres, Service, placement};
use isoloom_spec::syntax::{self, Code, EnvValue, FieldPath, Problem};
use serde_json::{Value, json};

use crate::lower::Scope;

/// Where a client finds a service's files, one folder per service.
pub const SERVICES_DIR: &str = "/etc/isoloom/services";

/// `SeaweedFS`'s release the self-hosted S3 server runs (`MinIO` no longer publishes builds), its
/// download and the SHA-256 it must have.
pub const SEAWEEDFS: (&str, &str) = (
    "https://github.com/seaweedfs/seaweedfs/releases/download/4.53/linux_amd64.tar.gz",
    "d1a7dec9f48d8b33895a0ea8951971defc2dc5312906dcc76fa5a5f74a6ee90d",
);

/// Apache Kafka's release the self-hosted broker runs, its download and the SHA-256 it must
/// have (the archive's own SHA-512 checked when it was pinned).
pub const KAFKA: (&str, &str) = (
    "https://archive.apache.org/dist/kafka/4.3.2/kafka_2.13-4.3.2.tgz",
    "023a48609152d9ca621709448e2db71ca080b96258447a7d39d2798c262a8526",
);

/// The region a self-hosted S3 server takes from its clients (it has none of its own).
const S3_REGION: &str = "us-east-1";

/// Where a client finds fact `field` of service `service` (`/etc/isoloom/services/db/password`).
#[must_use]
pub fn fact_path(service: &str, field: &str) -> String {
    format!("{SERVICES_DIR}/{service}/{field}")
}

/// A fact a client reads from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileFact {
    /// Its variable's suffix after the client's prefix (`BUCKET_FILE`).
    pub var: &'static str,
    /// Its file's name in the service's folder (`bucket`).
    pub file: &'static str,
    /// Whether it is a credential (owner-only, never in a generated file).
    pub secret: bool,
}

/// What a client of `service` reads from files, with `access`: the same names on every target.
#[must_use]
pub fn file_facts(service: &Service, access: Access) -> Vec<FileFact> {
    let fact = |var, file, secret| FileFact { var, file, secret };
    let keys = access == Access::Keys;
    match service {
        Service::Postgres(_) => vec![fact("PASSWORD_FILE", "password", true)],
        Service::ObjectStorage(_) => {
            let mut out = vec![
                fact("ENDPOINT_FILE", "endpoint", false),
                fact("BUCKET_FILE", "bucket", false),
                fact("REGION_FILE", "region", false),
                fact("API_FILE", "api", false),
            ];
            if keys {
                out.extend([fact("ACCESS_KEY_FILE", "access-key", true), fact("SECRET_KEY_FILE", "secret-key", true)]);
            } else {
                out.push(fact("IDENTITY_FILE", "identity", false));
            }
            out
        }
        Service::Kafka(_) => {
            let mut out = vec![
                fact("BOOTSTRAP_FILE", "bootstrap", false),
                fact("SECURITY_PROTOCOL_FILE", "security-protocol", false),
                fact("SASL_MECHANISM_FILE", "sasl-mechanism", false),
            ];
            if keys {
                out.extend([fact("USER_FILE", "user", true), fact("PASSWORD_FILE", "password", true)]);
            } else {
                out.push(fact("IDENTITY_FILE", "identity", false));
            }
            out
        }
    }
}

/// `source` as `scope` lowers it: its zone on, its services placed, their clients given their
/// facts.
pub(crate) fn source_for(source: &syntax::Spec, scope: Scope) -> Cow<'_, syntax::Spec> {
    if source.services.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut spec = source.clone();
    let dns = spec.dns.get_or_insert_with(EnvironmentDns::default);
    dns.private = true;
    let domain = dns.private_domain(&source.name).unwrap_or_default();
    let mut managed = IndexMap::new();
    for (name, service) in &source.services {
        let host = format!("{name}.{domain}");
        let placed = scope.target().map_or(Placement::Machine, |t| placement(service, t));
        let as_machine = placed == Placement::Machine;
        if as_machine && let Some(machine) = machine(name, &host, service) {
            spec.machines.insert(name.clone(), machine);
        } else {
            managed.insert(name.clone(), service.clone());
        }
        for (client, how) in service.clients() {
            let Some(m) = spec.machines.get_mut(client) else { continue };
            give(m, name, &host, service, how, as_machine);
            if as_machine && !m.depends_on.contains(name) {
                m.depends_on.push(name.clone());
            }
        }
    }
    spec.services = managed;
    Cow::Owned(spec)
}

/// Whether a spec with only services has something to run: its services become machines (or a
/// cloud's services), so it isn't empty.
#[must_use]
pub fn has_services(source: &syntax::Spec) -> bool {
    !source.services.is_empty()
}

/// The refusals of `target` for `source`'s services, each at its service.
pub(crate) fn refusals(source: &syntax::Spec, scope: Scope) -> Vec<Problem> {
    let Some(target) = scope.target() else { return Vec::new() };
    source
        .services
        .iter()
        .filter_map(|(name, s)| match placement(s, target) {
            Placement::Refused(why) => Some(Problem::new(
                Code::Refused,
                FieldPath::new("services").key(name),
                format!("{} can't run the {} service `{name}`: {why}", target.id(), s.kind()),
            )),
            _ => None,
        })
        .collect()
}

/// Service `name` (at `host`) given to client `m` as `how` says; its facts' files written here
/// when it runs as a machine (a cloud delivers them once applied).
fn give(m: &mut syntax::Machine, name: &str, host: &str, service: &Service, how: &Client, as_machine: bool) {
    let prefix = how.prefix(name);
    let mut env = |key: &str, value: String| {
        m.environment.insert(format!("{prefix}_{key}"), EnvValue::Text(value));
    };
    match service {
        Service::Postgres(p) => {
            env("HOST", host.to_owned());
            env("PORT", service.port().to_string());
            env("NAME", p.database.clone());
            env("USER", p.user.clone());
            // libpq's own default: TLS where the server offers it (every cloud), plain locally.
            env("SSLMODE", "prefer".to_owned());
        }
        Service::ObjectStorage(_) | Service::Kafka(_) => {
            env("AUTH", if how.access == Access::Identity { "identity" } else { "keys" }.to_owned());
        }
    }
    let facts = file_facts(service, how.access);
    for f in &facts {
        env(f.var, fact_path(name, f.file));
    }
    for f in &facts {
        let path = fact_path(name, f.file);
        // A database's password is the spec's secret on every platform; the rest is the
        // self-hosted server's, or what a cloud delivers.
        let file = match (secret_of(service, f.file), as_machine) {
            (Some(secret), true) => Some(json!({ "path": path, "secret": secret })),
            (Some(secret), false) if matches!(service, Service::Postgres(_)) => Some(json!({ "path": path, "secret": secret })),
            (None, true) => content_of(host, service, f.file).map(|c| json!({ "path": path, "content": c })),
            _ => None,
        };
        if let Some(file) = file.and_then(read) {
            m.files.push(file);
        }
    }
}

/// The spec secret behind credential file `file` of `service`, if one is.
fn secret_of<'a>(service: &'a Service, file: &str) -> Option<&'a str> {
    match (service, file) {
        (Service::Postgres(p), "password") => Some(&p.password),
        (Service::ObjectStorage(o), "access-key") => Some(&o.credentials.access_key),
        (Service::ObjectStorage(o), "secret-key") => Some(&o.credentials.secret_key),
        (Service::Kafka(k), "user") => Some(&k.credentials.user),
        (Service::Kafka(k), "password") => Some(&k.credentials.password),
        _ => None,
    }
}

/// What fact file `file` of the self-hosted `service` at `host` holds.
fn content_of(host: &str, service: &Service, file: &str) -> Option<String> {
    let port = service.port();
    let text = match file {
        "endpoint" => format!("http://{host}:{port}"),
        "bucket" => match service {
            Service::ObjectStorage(o) => o.bucket.clone(),
            _ => return None,
        },
        "region" => S3_REGION.to_owned(),
        "api" => "s3".to_owned(),
        "bootstrap" => format!("{host}:{port}"),
        "security-protocol" => "SASL_PLAINTEXT".to_owned(),
        "sasl-mechanism" => "SCRAM-SHA-512".to_owned(),
        _ => return None,
    };
    Some(text)
}

/// Service `name` (at `host`) as the machine that runs it.
fn machine(name: &str, host: &str, service: &Service) -> Option<syntax::Machine> {
    let mut m = match service {
        Service::Postgres(p) => postgres(p),
        Service::ObjectStorage(o) => object_storage(name, o),
        Service::Kafka(k) => kafka(name, host, k),
    };
    if service.publish().is_some()
        && let Some(ports) = m.get_mut("ports").and_then(Value::as_object_mut)
    {
        for port in ports.values_mut() {
            if let Some(port) = port.as_object_mut() {
                port.insert("publish".to_owned(), json!(true));
            }
        }
    }
    read(m)
}

/// The catalogue's `postgres`: the official image on containers, its packages on a Debian VM.
fn postgres(p: &Postgres) -> Value {
    json!({
        "os": "debian@12",
        "service": format!("postgres@{}", p.version),
        "with": { "database": p.database, "user": p.user, "password": { "secret": p.password } },
        "networks": p.networks,
        "ports": { "postgres": {} },
    })
}

/// A pinned download unpacked once: its steps.
fn unpacked((url, sha256): (&str, &str), archive: &str, unpack: &str, creates: &str) -> [Value; 2] {
    [
        json!({ "file": { "path": archive, "url": url, "sha256": sha256 } }),
        json!({ "command": unpack, "creates": creates }),
    ]
}

/// `SeaweedFS`'s server and its S3 gateway, checking the spec's keys (its configuration written
/// from their files at each start), its data in a volume. It creates a bucket on its first
/// upload, its identity being an admin.
fn object_storage(name: &str, o: &ObjectStorage) -> Value {
    let dir = fact_path(name, "");
    let start = format!(
        "set -eu\nmkdir -p /var/lib/seaweedfs /run/seaweedfs\numask 077\nprintf '{{\"identities\":[{{\"name\":\"app\",\"credentials\":[{{\"accessKey\":\"%s\",\"secretKey\":\"%s\"}}],\"actions\":[\"Admin\",\"Read\",\"Write\",\"List\",\"Tagging\"]}}]}}' \"$(cat {dir}access-key)\" \"$(cat {dir}secret-key)\" > /run/seaweedfs/s3.json\nexec /usr/local/bin/weed server -dir=/var/lib/seaweedfs -ip.bind=0.0.0.0 -s3 -s3.port=8333 -s3.config=/run/seaweedfs/s3.json"
    );
    let [download, unpack] = unpacked(
        SEAWEEDFS,
        "/opt/isoloom/seaweedfs.tar.gz",
        "tar -xzf /opt/isoloom/seaweedfs.tar.gz -C /usr/local/bin weed",
        "/usr/local/bin/weed",
    );
    json!({
        "os": "debian@12",
        "networks": o.networks,
        "ports": { "s3": { "port": 8333, "protocol": "tcp" } },
        "volumes": { format!("{name}-data"): "/var/lib/seaweedfs" },
        "resources": { "cpus": 1, "memory_mb": 1024 },
        "files": [
            { "path": fact_path(name, "access-key"), "secret": o.credentials.access_key },
            { "path": fact_path(name, "secret-key"), "secret": o.credentials.secret_key },
        ],
        "app": { "steps": [
            { "packages": ["ca-certificates", "curl"] },
            download,
            unpack,
            { "service": { "name": "seaweedfs", "run": ["/bin/sh", "-c", start] } },
        ] },
    })
}

/// Apache Kafka in `KRaft` mode, one node (broker and controller), its SASL/SCRAM login the spec's
/// secrets (formatted into its storage at its first start), its topics made once it answers.
fn kafka(name: &str, host: &str, k: &Kafka) -> Value {
    let dir = fact_path(name, "");
    let topics: Vec<String> = k.topics.iter().map(|(t, topic)| format!("{t}:{}", topic.partitions)).collect();
    let jaas = "org.apache.kafka.common.security.scram.ScramLoginModule required username=\"$u\" password=\"$p\";";
    let start = format!(
        "set -eu\nu=$(cat {dir}user); p=$(cat {dir}password)\nk=/var/lib/kafka; c=$k/server.properties\nmkdir -p $k/data\numask 077\ncat > $c <<EOF\nprocess.roles=broker,controller\nnode.id=1\ncontroller.quorum.voters=1@127.0.0.1:9093\nlisteners=SASL_PLAINTEXT://0.0.0.0:9092,CONTROLLER://127.0.0.1:9093\nadvertised.listeners=SASL_PLAINTEXT://{host}:9092\nlistener.security.protocol.map=SASL_PLAINTEXT:SASL_PLAINTEXT,CONTROLLER:PLAINTEXT\ncontroller.listener.names=CONTROLLER\ninter.broker.listener.name=SASL_PLAINTEXT\nsasl.enabled.mechanisms=SCRAM-SHA-512\nsasl.mechanism.inter.broker.protocol=SCRAM-SHA-512\nlistener.name.sasl_plaintext.scram-sha-512.sasl.jaas.config={jaas}\nlog.dirs=$k/data\noffsets.topic.replication.factor=1\ntransaction.state.log.replication.factor=1\ntransaction.state.log.min.isr=1\nauto.create.topics.enable=false\nEOF\ncat > $k/client.properties <<EOF\nsecurity.protocol=SASL_PLAINTEXT\nsasl.mechanism=SCRAM-SHA-512\nsasl.jaas.config={jaas}\nEOF\n[ -f $k/data/meta.properties ] || /opt/kafka/bin/kafka-storage.sh format -t \"$(/opt/kafka/bin/kafka-storage.sh random-uuid)\" -c $c --add-scram \"SCRAM-SHA-512=[name=$u,password=$p]\"\n(for t in {topics}; do n=${{t%%:*}}; n_p=${{t#*:}}; i=0; until /opt/kafka/bin/kafka-topics.sh --bootstrap-server 127.0.0.1:9092 --command-config $k/client.properties --create --if-not-exists --topic \"$n\" --partitions \"$n_p\" >/dev/null 2>&1; do i=$((i+1)); [ $i -lt 90 ] || break; sleep 2; done; done) &\nexport KAFKA_HEAP_OPTS='-Xms512m -Xmx512m'\nexec /opt/kafka/bin/kafka-server-start.sh $c",
        topics = topics.join(" "),
    );
    let [download, unpack] = unpacked(
        KAFKA,
        "/opt/isoloom/kafka.tgz",
        "mkdir -p /opt/kafka && tar -xzf /opt/isoloom/kafka.tgz -C /opt/kafka --strip-components=1",
        "/opt/kafka/bin/kafka-server-start.sh",
    );
    json!({
        "os": "debian@12",
        "networks": k.networks,
        "ports": { "kafka": { "port": 9092, "protocol": "tcp" } },
        "volumes": { format!("{name}-data"): "/var/lib/kafka" },
        "resources": { "cpus": 1, "memory_mb": 1536 },
        "files": [
            { "path": fact_path(name, "user"), "secret": k.credentials.user },
            { "path": fact_path(name, "password"), "secret": k.credentials.password },
        ],
        "app": { "steps": [
            { "packages": ["ca-certificates", "curl", "openjdk-17-jre-headless"] },
            download,
            unpack,
            { "service": { "name": "kafka", "run": ["/bin/sh", "-c", start] } },
        ] },
    })
}

/// A syntax value read from its JSON form (what [`syntax::parse`] reads from YAML).
fn read<T: serde::de::DeserializeOwned>(value: Value) -> Option<T> {
    serde_yaml_ng::to_value(value).ok().and_then(|v| serde_yaml_ng::from_value(v).ok())
}

#[cfg(test)]
mod tests;
