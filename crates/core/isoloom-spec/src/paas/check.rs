//! The checks `services:` needs beyond its types: its names, what it refers to (a network, the
//! secrets, the client machines), the private zone its names live in, and the values each
//! platform takes (a bucket's name, a login no platform reserves).

use super::model::{ObjectStorage, Postgres, Service};
use crate::contract::is_kebab;
use crate::syntax::{Code, FieldPath, Problem, Problems, Spec};

/// The major versions every platform offers for a Postgres service (and the catalogue pins).
pub(super) const POSTGRES_VERSIONS: &[&str] = &["16"];

/// Logins a platform keeps for itself (RDS, Azure, Cloud SQL, PostgreSQL), refused on all.
const RESERVED_USERS: &[&str] = &[
    "admin",
    "administrator",
    "azure_superuser",
    "azure_pg_admin",
    "guest",
    "public",
    "rdsadmin",
    "root",
    "cloudsqladmin",
];

/// The problems of `spec`'s services, added to `out`.
pub fn check(spec: &Spec, out: &mut Problems) {
    if spec.services.is_empty() {
        return;
    }
    if spec.dns.as_ref().and_then(|d| d.private_domain(&spec.name)).is_none() {
        out.push(
            Problem::new(
                Code::MissingField,
                FieldPath::new("dns").key("private"),
                "a service is reached by its name in the environment's private zone",
            )
            .fix("`dns: { private: true }`"),
        );
    }
    for (name, service) in &spec.services {
        let at = FieldPath::new("services").key(name);
        names(spec, &at, name, out);
        references(spec, &at, service, out);
        match service {
            Service::Postgres(p) => postgres(&at, p, out),
            Service::ObjectStorage(o) => object_storage(&at, o, out),
        }
    }
}

/// Its name: kebab case, not Isoloom's, not a machine's.
fn names(spec: &Spec, at: &FieldPath, name: &str, out: &mut Problems) {
    if !is_kebab(name) {
        out.push(Problem::new(
            Code::InvalidValue,
            at.clone(),
            format!("`{name}`: a service's name is kebab-case"),
        ));
    }
    if name.starts_with("isoloom-") {
        out.push(Problem::new(
            Code::InvalidValue,
            at.clone(),
            format!("`{name}`: names starting with `isoloom-` are Isoloom's own"),
        ));
    }
    if spec.machines.contains_key(name) {
        out.push(
            Problem::new(
                Code::Conflict,
                at.clone(),
                format!("`{name}` is a machine's name too: a service is one of the environment's names"),
            )
            .fix("rename one of them"),
        );
    }
}

/// Its network, its secrets and its clients, each declared.
fn references(spec: &Spec, at: &FieldPath, service: &Service, out: &mut Problems) {
    let networks = service.networks();
    if networks.len() != 1 {
        out.push(Problem::new(Code::InvalidValue, at.key("networks"), "a service answers on one network").fix("`networks: { lab: 40 }`"));
    }
    for net in networks.keys() {
        if !spec.networks.contains_key(net) {
            out.push(unknown(&at.key("networks").key(net), "network", net, spec.networks.keys()));
        }
    }
    for (field, secret) in service.secrets() {
        if !spec.secrets.contains_key(secret) {
            let path = field.split('.').fold(at.clone(), |p, k| p.key(k));
            out.push(unknown(&path, "secret", secret, spec.secrets.keys()));
        }
    }
    for (machine, client) in service.clients() {
        let here = at.key("clients").key(machine);
        if !spec.machines.contains_key(machine) {
            out.push(unknown(&here, "machine", machine, spec.machines.keys()));
        }
        if let Some(prefix) = &client.env_prefix
            && !upper_snake(prefix)
        {
            out.push(Problem::new(
                Code::InvalidValue,
                here.key("env_prefix"),
                format!("`{prefix}`: a prefix is upper snake case (`DATABASE`)"),
            ));
        }
    }
}

/// A Postgres service's version and login.
fn postgres(at: &FieldPath, p: &Postgres, out: &mut Problems) {
    if !POSTGRES_VERSIONS.contains(&p.version.as_str()) {
        out.push(
            Problem::new(
                Code::InvalidValue,
                at.key("version"),
                format!("`{}`: not a version every platform offers", p.version),
            )
            .fix(format!(
                "one of {}",
                POSTGRES_VERSIONS.iter().map(|v| format!("`\"{v}\"`")).collect::<Vec<_>>().join(", ")
            )),
        );
    }
    for (field, value) in [("database", &p.database), ("user", &p.user)] {
        if !identifier(value) {
            out.push(Problem::new(
                Code::InvalidValue,
                at.key(field),
                format!("`{value}`: lowercase letters, digits and `_`, starting with a letter, 63 at most"),
            ));
        }
    }
    if RESERVED_USERS.contains(&p.user.as_str()) || p.user.starts_with("pg_") {
        out.push(
            Problem::new(
                Code::InvalidValue,
                at.key("user"),
                format!("`{}`: a platform keeps this login for itself", p.user),
            )
            .fix("`user: app`"),
        );
    }
}

/// An object-storage service's bucket name.
fn object_storage(at: &FieldPath, o: &ObjectStorage, out: &mut Problems) {
    let b = &o.bucket;
    let ok =
        (3..=40).contains(&b.len()) && b.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-') && !b.starts_with('-') && !b.ends_with('-');
    if !ok {
        out.push(Problem::new(
            Code::InvalidValue,
            at.key("bucket"),
            format!("`{b}`: 3 to 40 lowercase letters, digits and `-` (a cloud adds the copy's 23-character prefix)"),
        ));
    }
}

/// A name `what` that names nothing declared, with the declared ones as the fix.
fn unknown<'a>(at: &FieldPath, what: &str, name: &str, known: impl Iterator<Item = &'a String>) -> Problem {
    let known: Vec<String> = known.map(|k| format!("`{k}`")).collect();
    let p = Problem::new(Code::UnknownReference, at.clone(), format!("no {what} is named `{name}`"));
    if known.is_empty() {
        p.fix(format!("declare the {what}"))
    } else {
        p.fix(format!("declared: {}", known.join(", ")))
    }
}

/// A SQL identifier every platform takes as it is.
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && s.len() <= 63 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `DATABASE`, `S3_FILES`.
fn upper_snake(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}
