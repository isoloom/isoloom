//! The platform level on the clouds (`services:`, the `paas` feature): each service the plan
//! keeps managed, as the cloud's own resources in `services.tf` beside the module's `main.tf`,
//! which they reference by address (the VPC, the subnets, the private zone, `local.name`).
//!
//! - A database is reached at its name in the private zone (the spec's services turned the zone
//!   on, which `services.tf` says, with what the zone costs); its password is the spec secret's
//!   variable, the one the client's file reads.
//! - What a client reads from files and only the cloud knows (a bucket's name, the brokers, the
//!   keys it issues) is delivered once applied ([`delivery`]): over SSH to each client, after its
//!   set-up, into the same files the self-hosted targets write.
//! - A client with `access: identity` runs as an identity of its own ([`attach`] gives its
//!   instance that identity in `main.tf`), allowed on the services it uses: no key at all.
//!
//! AWS, Azure and Google Cloud render them; the other clouds refuse a spec with services.

mod aws;
mod azure;
mod delivery;
mod gcp;

use isoloom_plan::Environment;
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::contract::{OUTPUT_DIR, secret_variable};
use isoloom_spec::model::Target;
use isoloom_spec::paas::{Access, Api, Service, Size};

use crate::cloud::Cloud;
use crate::hcl_builder::{Expression, Module, Template, reference};
use crate::module::ident;
use crate::module::variable::Variable;
use crate::{Error, Result};

/// Why `cloud` can't render `env`'s managed services, if it can't.
pub(crate) fn refusal(cloud: Cloud, env: &Environment) -> Option<String> {
    let services = &env.spec().services;
    if services.is_empty() {
        return None;
    }
    if !matches!(cloud, Cloud::Aws | Cloud::Azure | Cloud::Gcp) {
        return Some(format!(
            "{}'s managed services aren't rendered (`services:`): only AWS's, Azure's and GCP's are",
            cloud.name()
        ));
    }
    for (name, s) in services {
        if let Some(limit) = isoloom_spec::paas::cloud_limits(name, s).into_iter().find(|l| l.cloud == cloud.id()) {
            let p = limit.problem;
            return Some(p.fix.as_ref().map_or_else(|| p.line(), |fix| format!("{}: {fix}", p.line())));
        }
        if cloud == Cloud::Azure
            && let Service::ObjectStorage(o) = s
            && o.api == Api::S3
        {
            return Some(format!(
                "Azure Blob Storage has no S3 API (service `{name}`): `api: platform`, or another cloud"
            ));
        }
        if let Some(client) = s.clients().keys().find(|c| env.vm(c).is_some_and(|vm| vm.plan.is_windows())) {
            return Some(format!(
                "a client's service files are delivered over SSH, and the Windows machine `{client}` takes WinRM (service `{name}`)"
            ));
        }
    }
    None
}

/// One service, as its cloud writes it.
pub(crate) struct Each<'a> {
    /// Its name.
    name: &'a str,
    /// What it is.
    service: &'a Service,
    /// Its name in the private zone (`db.shop.internal`).
    host: String,
    /// The network it answers on.
    network: &'a str,
}

/// The services of `env`, each with its name in `domain`.
fn each<'a>(env: &'a Environment, domain: &str) -> Vec<Each<'a>> {
    env.spec()
        .services
        .iter()
        .map(|(name, service)| Each {
            name,
            service,
            host: format!("{name}.{domain}"),
            network: service.networks().keys().next().map_or("", String::as_str),
        })
        .collect()
}

/// What the private zone costs on `cloud`, a month (list prices, for the notice).
fn zone_price(cloud: Cloud) -> &'static str {
    match cloud {
        Cloud::Azure => "an Azure private DNS zone, about $0.50 a month",
        Cloud::Gcp => "a Cloud DNS private zone, about $0.20 a month",
        _ => "a Route 53 private hosted zone, about $0.50 a month",
    }
}

/// `cloud`'s `services.tf` for `env`, when it has managed services.
///
/// # Errors
///
/// [`Error::Module`] when the file can't be written.
pub(crate) fn file(cloud: Cloud, env: &Environment) -> Result<Option<GeneratedFile>> {
    let Some(zone) = &env.private_dns else { return Ok(None) };
    if env.spec().services.is_empty() {
        return Ok(None);
    }
    let folder = format!("{}/{}", Target::CloudVm.id(), cloud.id());
    let mut m = Module::new(format!(
        "{}# The environment's services (`services:`), beside main.tf, whose network, zone and names they use.\n# `dns.private` is on: a spec with services turns it on, as each service is reached by its name\n# in the zone ({}). That zone is billed: {}.\n",
        header("#"),
        zone.domain,
        zone_price(cloud)
    ));
    for v in variables(cloud, env) {
        m.block(v.block());
    }
    let services = each(env, &zone.domain);
    shared(cloud, &services, &mut m);
    for at in &services {
        match cloud {
            Cloud::Aws => aws::write(env, at, &mut m),
            Cloud::Azure => azure::write(at, &mut m),
            _ => gcp::write(at, &mut m),
        }
    }
    identities(cloud, env, &services, &mut m);
    for at in &services {
        delivery::write(cloud, env, at, &mut m);
    }
    let contents = m.render().map_err(|source| Error::Module {
        target: Target::CloudVm,
        folder: folder.clone(),
        source,
    })?;
    Ok(Some(GeneratedFile::new(format!("{OUTPUT_DIR}/{folder}/services.tf"), contents)))
}

/// What several services share on `cloud`: AWS's subnets, Google Cloud's APIs and private
/// services access.
fn shared(cloud: Cloud, services: &[Each<'_>], m: &mut Module) {
    let all: Vec<&Service> = services.iter().map(|at| at.service).collect();
    match cloud {
        Cloud::Aws if aws::needs_subnets(&all) => {
            let published = services
                .iter()
                .any(|at| matches!(at.service, Service::Postgres(_)) && at.service.publish().is_some());
            aws::subnets(published, m);
        }
        Cloud::Gcp => {
            gcp::apis(&all, m);
            let mut nets: Vec<&str> = Vec::new();
            for at in services.iter().filter(|at| matches!(at.service, Service::Postgres(_))) {
                if !nets.contains(&at.network) {
                    nets.push(at.network);
                    gcp::private_access(at.network, m);
                }
            }
        }
        _ => {}
    }
}

/// Each client machine with `access: identity`: its identity, allowed on the services it uses.
fn identities(cloud: Cloud, env: &Environment, services: &[Each<'_>], m: &mut Module) {
    for machine in identity_clients(env) {
        let used: Vec<&Each<'_>> = services
            .iter()
            .filter(|at| at.service.clients().get(machine).is_some_and(|c| c.access == Access::Identity))
            .collect();
        match cloud {
            Cloud::Aws => aws::identity(machine, &used, m),
            Cloud::Azure => azure::identity(machine, &used, m),
            _ => gcp::identity(machine, &used, m),
        }
    }
}

/// The VMs that are a client with `access: identity` of some service, in spec order.
fn identity_clients(env: &Environment) -> Vec<&str> {
    env.vms()
        .map(|vm| vm.name)
        .filter(|m| {
            env.spec()
                .services
                .values()
                .any(|s| s.clients().get(*m).is_some_and(|c| c.access == Access::Identity))
        })
        .collect()
}

/// Gives each identity client's instance in `m` (the module's `main.tf`) its identity.
pub(crate) fn attach(cloud: Cloud, env: &Environment, m: &mut Module) {
    let (kind, structures): (&str, fn(&str) -> Vec<hcl::Structure>) = match cloud {
        Cloud::Aws => (aws::INSTANCE, aws::attach),
        Cloud::Azure => (azure::INSTANCE, azure::attach),
        Cloud::Gcp => (gcp::INSTANCE, gcp::attach),
        _ => return,
    };
    for machine in identity_clients(env) {
        let id = ident(machine);
        for b in m.blocks_mut() {
            let labels: Vec<&str> = b.labels.iter().map(hcl::BlockLabel::as_str).collect();
            if b.identifier.as_str() == "resource" && labels == [kind, id.as_str()] {
                b.body.0.extend(structures(machine));
            }
        }
    }
}

/// The variables `services.tf` reads that `main.tf` doesn't declare: the spec secrets a cloud
/// takes (a database's password; MSK's SCRAM login) and the allow-lists of published databases.
fn variables(cloud: Cloud, env: &Environment) -> Vec<Variable> {
    let mut named: Vec<String> = crate::module::inputs::variables_of(env)
        .into_iter()
        .filter_map(|v| v.block().labels().first().map(|l| l.as_str().to_owned()))
        .collect();
    let mut out = Vec::new();
    let mut add = |name: String, v: Variable| {
        if !named.contains(&name) {
            named.push(name);
            out.push(v);
        }
    };
    for s in env.spec().services.values() {
        let secrets: Vec<&str> = match s {
            Service::Postgres(p) => vec![&p.password],
            Service::Kafka(k) if cloud == Cloud::Aws => vec![&k.credentials.user, &k.credentials.password],
            _ => Vec::new(),
        };
        for name in secrets {
            let var = secret_variable(name);
            let v = Variable::new(&var, "string")
                .sensitive()
                .describe(&format!("The secret `{name}` (the spec's `secrets`), which a service takes"));
            add(var, v);
        }
        if let (Service::Postgres(_), Some(p)) = (s, s.publish()) {
            let v = Variable::new(&p.allow, "string")
                .describe("Who may reach the published services, a CIDR (the spec's `inputs`): the player's address, e.g. 203.0.113.7/32");
            add(p.allow.clone(), v);
        }
    }
    out
}

/// The blocks a service's own ingress lets in, as the spec defines its access and nothing
/// else: its network's, and those of the networks `reach` lets reach it. (Published, its
/// allow-list too, which the clouds add themselves.)
fn reached_from(env: &Environment, at: &Each<'_>) -> Vec<String> {
    let spec = env.spec();
    let nets = std::iter::once(at.network).chain(spec.reach.iter().filter(|r| r.to == at.network).map(|r| r.from.as_str()));
    let mut out: Vec<String> = Vec::new();
    for net in nets {
        if let Some(plan) = env.networks.get(net) {
            let block = plan.declared_cidr.to_string();
            if !out.contains(&block) {
                out.push(block);
            }
        }
    }
    out
}

/// The variable holding secret `name`'s value.
fn secret(name: &str) -> Expression {
    reference(&format!("var.{}", secret_variable(name)))
}

/// `"${local.name}-<suffix>"`: a name unique to the copy.
fn named(suffix: &str) -> Expression {
    Template::new()
        .with_expr(reference("local.name"))
        .with_text(format!("-{suffix}"))
        .into_expression()
}

/// `"isoloom-${terraform_data.id.output}-<bucket>"`: a bucket name, global on S3 and GCS, short
/// enough for any environment's name.
fn bucket_name(bucket: &str) -> Expression {
    Template::text("isoloom-")
        .with_expr(reference("terraform_data.id.output"))
        .with_text(format!("-{bucket}"))
        .into_expression()
}

/// Each cloud's tier for a portable size: (AWS, Azure, Google Cloud).
fn tier(size: Size) -> (&'static str, &'static str, &'static str) {
    match size {
        Size::Small => ("db.t4g.micro", "B_Standard_B1ms", "db-f1-micro"),
        Size::Medium => ("db.t4g.medium", "GP_Standard_D2ds_v5", "db-custom-2-7680"),
    }
}

#[cfg(test)]
mod tests;
