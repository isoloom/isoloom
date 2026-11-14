//! The platform level on the clouds (`services:`, the `paas` feature): each service the plan
//! keeps managed, as the cloud's own resources in `services.tf` beside the module's `main.tf`,
//! which they reference by address (the VPC, the subnets, the private zone, `local.name`). A
//! service is reached at its name in the private zone, so each one adds its record there; its
//! password is the spec secret's variable, the one the machines' files read.
//!
//! AWS, Azure and Google Cloud render them; the other clouds refuse a spec with services, as
//! Azure refuses a bucket whose clients speak S3.

mod aws;
mod azure;
mod gcp;

use isoloom_plan::Environment;
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::contract::{OUTPUT_DIR, secret_variable};
use isoloom_spec::model::Target;
use isoloom_spec::paas::{Api, Service, Size};

use crate::cloud::Cloud;
use crate::hcl_builder::{Expression, Module, Template, reference};
use crate::module::variable::Variable;
use crate::{Error, Result};

/// Why `cloud` can't render `env`'s managed services, if it can't.
pub(crate) fn refusal(cloud: Cloud, env: &Environment) -> Option<String> {
    let services = &env.spec().services;
    if services.is_empty() {
        return None;
    }
    match cloud {
        Cloud::Aws | Cloud::Gcp => None,
        Cloud::Azure => services.iter().find_map(|(name, s)| match s {
            Service::ObjectStorage(o) if o.api == Api::S3 => Some(format!(
                "Azure Blob Storage has no S3 API (service `{name}`): `api: platform`, or another cloud"
            )),
            _ => None,
        }),
        _ => Some(format!("{}'s managed services aren't rendered yet (`services:`)", cloud.name())),
    }
}

/// `cloud`'s `services.tf` for `env`, when it has managed services.
///
/// # Errors
///
/// [`Error::Module`] when the file can't be written.
pub(crate) fn file(cloud: Cloud, env: &Environment) -> Result<Option<GeneratedFile>> {
    let services = &env.spec().services;
    let Some(zone) = &env.private_dns else { return Ok(None) };
    if services.is_empty() {
        return Ok(None);
    }
    let folder = format!("{}/{}", Target::CloudVm.id(), cloud.id());
    let mut m = Module::new(format!(
        "{}# The environment's services (`services:`), beside main.tf, whose network, zone and names they use.\n",
        header("#")
    ));
    for v in secret_variables(env) {
        m.block(v.block());
    }
    let all: Vec<&Service> = services.values().collect();
    match cloud {
        Cloud::Aws if all.iter().any(|s| matches!(s, Service::Postgres(_))) => aws::subnets(&mut m),
        Cloud::Gcp => gcp::apis(&all, &mut m),
        _ => {}
    }
    for (name, service) in services {
        let at = Each {
            name,
            service,
            host: format!("{name}.{}", zone.domain),
            network: service.networks().keys().next().map_or("", String::as_str),
        };
        match cloud {
            Cloud::Aws => aws::write(env, &at, &mut m),
            Cloud::Azure => azure::write(&at, &mut m),
            Cloud::Gcp => gcp::write(&at, &mut m),
            _ => {}
        }
    }
    let contents = m.render().map_err(|source| Error::Module {
        target: Target::CloudVm,
        folder: folder.clone(),
        source,
    })?;
    Ok(Some(GeneratedFile::new(format!("{OUTPUT_DIR}/{folder}/services.tf"), contents)))
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

/// The secrets the services read whose variables `main.tf` doesn't declare (no machine gets
/// them as a file there), each once.
fn secret_variables(env: &Environment) -> Vec<Variable> {
    let mut named: Vec<String> = crate::module::inputs::variables_of(env)
        .into_iter()
        .filter_map(|v| v.block().labels().first().map(|l| l.as_str().to_owned()))
        .collect();
    let mut out = Vec::new();
    for s in env.spec().services.values() {
        if let Service::Postgres(p) = s {
            let var = secret_variable(&p.password);
            if !named.contains(&var) {
                out.push(
                    Variable::new(&var, "string")
                        .sensitive()
                        .describe(&format!("The secret `{}` (the spec's `secrets`), a service's password", p.password)),
                );
                named.push(var);
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
