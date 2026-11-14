//! The platform level lowered (`services:`, the `paas` feature): before a scope lowers the spec,
//! each service a target runs as a machine becomes one (the catalogue's `postgres`, an S3 server's
//! container), each client machine gets the service's facts through what machines already have
//! (environment text values; credentials as secret files), and a target that can't render a
//! service refuses it at its field. What a cloud renders as its managed service stays in
//! `services`, for its Terraform backend.
//!
//! A service's address is its name in the environment's private zone (`<service>.<domain>`):
//! known when generating on every target, so a client never needs a value only an `apply` knows.

use std::borrow::Cow;

use indexmap::IndexMap;
use isoloom_spec::paas::{ObjectStorage, Placement, Postgres, Service, placement};
use isoloom_spec::syntax::{self, Code, EnvValue, FieldPath, Problem};
use serde_json::{Value, json};

use crate::lower::Scope;

/// The self-hosted S3 server's image: SeaweedFS's S3 gateway (MinIO no longer publishes builds).
pub const S3_SERVER_IMAGE: &str = "chrislusf/seaweedfs:4.53";

/// The port the self-hosted S3 server answers on.
pub const S3_PORT: u16 = 8333;

/// The port a Postgres service answers on, on every platform.
pub const POSTGRES_PORT: u16 = 5432;

/// The region a self-hosted S3 server takes from its clients (it has none of its own).
const S3_REGION: &str = "us-east-1";

/// Where a client finds service `service`'s secret `field` (`/run/isoloom/services/db/password`).
#[must_use]
pub fn secret_path(service: &str, field: &str) -> String {
    format!("/run/isoloom/services/{service}/{field}")
}

/// `source` as `scope` lowers it: its services placed, their clients given their facts.
pub(crate) fn source_for(source: &syntax::Spec, scope: Scope) -> Cow<'_, syntax::Spec> {
    if source.services.is_empty() {
        return Cow::Borrowed(source);
    }
    let mut spec = source.clone();
    let host = |name: &str| match source.dns.as_ref().and_then(|d| d.private_domain(&source.name)) {
        Some(domain) => format!("{name}.{domain}"),
        None => name.to_owned(),
    };
    let mut managed = IndexMap::new();
    for (name, service) in &source.services {
        let placed = scope.target().map_or(Placement::Machine, |t| placement(service, t));
        if placed == Placement::Machine
            && let Some(machine) = machine(name, service)
        {
            spec.machines.insert(name.clone(), machine);
        } else {
            managed.insert(name.clone(), service.clone());
        }
        for (client, how) in service.clients() {
            let Some(m) = spec.machines.get_mut(client) else { continue };
            give(m, &how.prefix(name), &facts(name, &host(name), service));
            if placed == Placement::Machine && !m.depends_on.contains(name) {
                m.depends_on.push(name.clone());
            }
        }
    }
    spec.services = managed;
    Cow::Owned(spec)
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

/// What a client of a service gets.
struct Facts {
    /// Environment values, after the client's prefix (`HOST` → `DATABASE_HOST`).
    env: Vec<(&'static str, String)>,
    /// Secret files: the variable naming the path, the path, the spec secret.
    files: Vec<(&'static str, String, String)>,
}

/// The facts of service `name` at `host`.
fn facts(name: &str, host: &str, service: &Service) -> Facts {
    match service {
        Service::Postgres(p) => Facts {
            env: vec![
                ("HOST", host.to_owned()),
                ("PORT", POSTGRES_PORT.to_string()),
                ("NAME", p.database.clone()),
                ("USER", p.user.clone()),
                // libpq's own default: TLS where the server offers it (every cloud), plain locally.
                ("SSLMODE", "prefer".to_owned()),
            ],
            files: vec![("PASSWORD_FILE", secret_path(name, "password"), p.password.clone())],
        },
        Service::ObjectStorage(o) => Facts {
            env: vec![
                ("ENDPOINT", format!("http://{host}:{S3_PORT}")),
                ("BUCKET", o.bucket.clone()),
                ("REGION", S3_REGION.to_owned()),
                ("API", "s3".to_owned()),
            ],
            files: vec![
                ("ACCESS_KEY_FILE", secret_path(name, "access-key"), o.credentials.access_key.clone()),
                ("SECRET_KEY_FILE", secret_path(name, "secret-key"), o.credentials.secret_key.clone()),
            ],
        },
    }
}

/// `facts` given to machine `m` under `prefix`.
fn give(m: &mut syntax::Machine, prefix: &str, facts: &Facts) {
    for (key, value) in &facts.env {
        m.environment.insert(format!("{prefix}_{key}"), EnvValue::Text(value.clone()));
    }
    for (key, path, secret) in &facts.files {
        m.environment.insert(format!("{prefix}_{key}"), EnvValue::Text(path.clone()));
        if let Some(file) = read(json!({ "path": path, "secret": secret })) {
            m.files.push(file);
        }
    }
}

/// Service `name` as the machine that runs it.
fn machine(name: &str, service: &Service) -> Option<syntax::Machine> {
    read(match service {
        Service::Postgres(p) => postgres(p),
        Service::ObjectStorage(o) => object_storage(name, o),
    })
}

/// The catalogue's `postgres`: the official image on containers, its packages on a Debian VM.
fn postgres(p: &Postgres) -> Value {
    json!({
        "os": "debian@12",
        "service": format!("postgres@{}", p.version),
        "with": { "database": p.database, "user": p.user, "password": { "secret": p.password } },
        "networks": p.networks,
    })
}

/// SeaweedFS's server with its S3 gateway, its data in a volume. It runs without authentication
/// (its clients' keys are accepted, not checked: the draft's known gap) and creates a bucket on
/// its first upload.
fn object_storage(name: &str, o: &ObjectStorage) -> Value {
    json!({
        "app": { "image": S3_SERVER_IMAGE },
        "command": ["server", "-s3", format!("-s3.port={S3_PORT}")],
        "networks": o.networks,
        "ports": { "s3": { "port": S3_PORT, "protocol": "tcp" } },
        "volumes": { format!("{name}-data"): "/data" },
        // Its own keys, where an authenticated server's configuration will read them.
        "files": [
            { "path": secret_path(name, "access-key"), "secret": o.credentials.access_key },
            { "path": secret_path(name, "secret-key"), "secret": o.credentials.secret_key },
        ],
    })
}

/// A syntax value read from its JSON form (what [`syntax::parse`] reads from YAML).
fn read<T: serde::de::DeserializeOwned>(value: Value) -> Option<T> {
    serde_yaml_ng::to_value(value).ok().and_then(|v| serde_yaml_ng::from_value(v).ok())
}

#[cfg(test)]
mod tests;
