//! The services in the snapshot (`services`, the `paas` feature): what a program reads of each, as
//! it reads a machine's. Its kind, its name in the zone (its Service name on Kubernetes) and its port; how each target renders it
//! (`machine`: a machine of the same name, in `machines`, with its published port there;
//! `managed`: the cloud's own, its facts in the module's outputs; or why it is refused); its
//! allow-list input when published; and each client's contract: its environment, the files it
//! reads, the same on every target, and the helper it waits for them with.

use isoloom_plan::Lab;
use isoloom_plan::paas::{WAIT_DEFAULT_S, WAIT_HELPER, fact_path, file_facts};
use isoloom_spec::model::Target;
use isoloom_spec::paas::{Access, Placement, Service, placement};
use serde_json::{Map, Value, json};

/// The snapshot's `services` and `services_zone` entries.
pub(super) fn entries(lab: &Lab) -> Vec<(&'static str, Value)> {
    let source = lab.source();
    if source.services.is_empty() {
        return Vec::new();
    }
    let spec = lab.spec();
    let domain = spec.private_domain().unwrap_or_default();
    let written = source.dns.as_ref().is_some_and(|d| d.private);
    let services: Map<String, Value> = source
        .services
        .iter()
        .map(|(name, s)| (name.clone(), service(&spec, name, &format!("{name}.{domain}"), s)))
        .collect();
    vec![
        ("services", services.into()),
        ("services_zone", json!({ "domain": domain, "turned_on_by_services": !written })),
    ]
}

/// Service `name` at `host`.
fn service(spec: &isoloom_spec::model::Spec, name: &str, host: &str, s: &Service) -> Value {
    let targets: Map<String, Value> = Target::ALL
        .into_iter()
        .map(|t| {
            let how = match placement(s, t) {
                Placement::Machine => json!("machine"),
                Placement::Managed => json!("managed"),
                Placement::Refused(why) => json!({ "refused": why }),
            };
            (t.id().to_owned(), how)
        })
        .collect();
    let clients: Map<String, Value> = s
        .clients()
        .iter()
        .map(|(client, how)| {
            let prefix = format!("{}_", how.prefix(name));
            let environment: Map<String, Value> = spec
                .machines
                .get(client)
                .into_iter()
                .flat_map(|m| m.environment.iter())
                .filter(|(k, _)| k.starts_with(&prefix))
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            let files: Vec<Value> = file_facts(s, how.access)
                .into_iter()
                .map(|f| json!({ "variable": format!("{prefix}{}", f.var), "path": fact_path(name, f.file), "credential": f.secret }))
                .collect();
            let access = if how.access == Access::Identity { "identity" } else { "keys" };
            // On a cloud a service's files land after the client's set-up: it waits for each.
            let wait = matches!(s, Service::ObjectStorage(_) | Service::Kafka(_))
                .then(|| json!({ "helper": WAIT_HELPER, "default_timeout_s": WAIT_DEFAULT_S, "usage": "wait-for FILE [SECONDS]" }));
            (
                client.clone(),
                json!({ "access": access, "environment": environment, "files": files, "wait": wait }),
            )
        })
        .collect();
    json!({
        "kind": s.kind(),
        "host": host,
        // A cluster takes no zone of Isoloom's: there, its Service name.
        "host_on_kubernetes": name,
        "port": s.port(),
        "network": s.networks().keys().next(),
        "published": s.publish().map(|p| json!({ "allow_input": p.allow })),
        "targets": targets,
        "clients": clients,
    })
}
