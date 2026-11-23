//! A service's facts delivered once applied: what a client reads from files and only the cloud
//! knows (a bucket's name, the brokers, the keys it issues), written over SSH into each client's
//! `/etc/isoloom/services/<service>/`, the files the self-hosted targets write at set-up. One
//! `terraform_data` per service and client, after the client's set-up (its readiness wait), made
//! again when the client or a value changes. A credential's file is owner-only (`0400`), the
//! others readable (`0444`); each value goes up as a file of its own, never on a command line.
//!
//! A client that reads its files at boot finds them once the service exists: on a cloud, after
//! its own set-up (the contract says to wait for a `_FILE` to appear).

use isoloom_plan::Environment;
use isoloom_plan::paas::{fact_path, file_facts};
use isoloom_spec::shell::quote;

use super::{Each, aws, azure, gcp};
use crate::cloud::Cloud;
use crate::hcl_builder::{Block, Expression, Module, call, list, reference, string};
use crate::module::{ResourceRef, ident};

/// The deliveries of service `at` on `cloud`, one per client that has files to get.
pub(super) fn write(cloud: Cloud, env: &Environment, at: &Each<'_>, m: &mut Module) {
    for (client, how) in at.service.clients() {
        let Some(vm) = env.vm(client) else { continue };
        let facts: Vec<(String, bool, Expression)> = file_facts(at.service, how.access)
            .into_iter()
            .filter_map(|f| {
                let value = match cloud {
                    Cloud::Aws => aws::fact(at, f.file, how.access, client),
                    Cloud::Azure => azure::fact(at, f.file, how.access, client),
                    _ => gcp::fact(at, f.file, how.access, client),
                }?;
                Some((f.file.to_owned(), f.secret, value))
            })
            .collect();
        if facts.is_empty() {
            continue;
        }
        let (host, instance) = match cloud {
            Cloud::Aws => (aws::public_ip(env, client), format!("{}.{}.id", aws::INSTANCE, ident(client))),
            Cloud::Azure => (azure::public_ip(client), format!("{}.{}.id", azure::INSTANCE, ident(client))),
            _ => (gcp::public_ip(client), format!("{}.{}.id", gcp::INSTANCE, ident(client))),
        };
        let user = match cloud {
            Cloud::Aws => (crate::cloud::aws::BASE.user)(vm.os),
            Cloud::Azure => (crate::cloud::azure::BASE.user)(vm.os),
            _ => (crate::cloud::gcp::BASE.user)(vm.os),
        };
        m.block(block(at.name, client, &facts, (host, user, &instance)));
    }
}

/// The delivery of `facts` (file, whether a credential, value) of service `service` to `client`,
/// reached at `host` as `user`, its instance `instance`.
fn block(service: &str, client: &str, facts: &[(String, bool, Expression)], (host, user, instance): (Expression, &str, &str)) -> Block {
    let tmp = |file: &str| format!("/tmp/isoloom-service-{service}-{file}");
    let dir = fact_path(service, "");
    let mut steps = vec![format!("install -d -m 0755 {}", quote(dir.trim_end_matches('/')))];
    for (file, secret, _) in facts {
        let mode = if *secret { "0400" } else { "0444" };
        let (from, to) = (quote(&tmp(file)), quote(&fact_path(service, file)));
        steps.push(format!("install -m {mode} {from} {to} && rm -f {from}"));
    }
    let install = steps.join(" && ");
    let values = list(facts.iter().map(|(_, _, v)| v.clone()));
    let connection = Block::builder("connection")
        .add_attribute(("type", string("ssh")))
        .add_attribute(("host", host))
        .add_attribute(("user", string(user)))
        .add_attribute(("private_key", call("file", [call("pathexpand", [reference("var.ssh_private_key_file")])])))
        .add_attribute(("timeout", string("10m")))
        .build();
    let mut b = ResourceRef::new("terraform_data", &format!("isoloom_service_{service}_to_{client}"))
        .block()
        .add_attribute(("triggers_replace", list([reference(instance), call("sha256", [call("jsonencode", [values])])])))
        .add_block(connection);
    for (file, _, value) in facts {
        b = b.add_block(
            Block::builder("provisioner")
                .add_label("file")
                .add_attribute(("content", value.clone()))
                .add_attribute(("destination", string(tmp(file))))
                .build(),
        );
    }
    b.add_block(
        Block::builder("provisioner")
            .add_label("remote-exec")
            .add_attribute(("inline", list([string(format!("sudo sh -c {}", quote(&install)))])))
            .build(),
    )
    .add_attribute(("depends_on", list([reference(&format!("terraform_data.isoloom_ready_{}", ident(client)))])))
    .build()
}
