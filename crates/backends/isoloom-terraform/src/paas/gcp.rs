//! The services on Google Cloud: Cloud SQL for Postgres at a private address (private services
//! access: a range of the service's VPC network peered with Google's), a Cloud Storage bucket for
//! object storage (S3 through its XML API's interoperability). Each service a client reaches has
//! an A record in the private zone. The APIs they need are turned on in the project Isoloom
//! makes, as `main.tf` turns on Compute's.

use isoloom_spec::paas::{ObjectStorage, Postgres, Service};

use super::{Each, bucket_name, named, secret, tier};
use crate::hcl_builder::{BinaryOperator, Block, BlockBuilder, Expression, Module, Template, binary, boolean, conditional, list, number, reference, string};
use crate::module::{ResourceRef, ident, output};

/// Service `at` on Google Cloud.
pub(super) fn write(at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
    }
}

/// The APIs the services need, turned on once each (in the project Isoloom makes; a given
/// project's own are its owner's).
pub(super) fn apis(services: &[&Service], m: &mut Module) {
    let mut wanted: Vec<&str> = Vec::new();
    for s in services {
        let needs: &[&str] = match s {
            Service::Postgres(_) => &["sqladmin", "servicenetworking"],
            Service::ObjectStorage(_) => &["storage"],
        };
        for api in needs {
            if !wanted.contains(api) {
                wanted.push(api);
            }
        }
    }
    for api in wanted {
        m.block(
            ResourceRef::new("google_project_service", api)
                .block()
                .add_attribute((
                    "count",
                    conditional(binary(reference("var.project"), BinaryOperator::Eq, string("")), number(1), number(0)),
                ))
                .add_attribute(("project", reference("local.project")))
                .add_attribute(("service", string(format!("{api}.googleapis.com"))))
                .add_attribute(("disable_on_destroy", boolean(false)))
                .build(),
        );
    }
}

/// In the project.
fn project(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("project", reference("local.project")))
}

/// The peering of the service's network with Google's services, once per network.
fn private_access(at: &Each<'_>, m: &mut Module) -> ResourceRef {
    let net = format!("google_compute_network.{}.id", ident(at.network));
    let range = ResourceRef::new("google_compute_global_address", &format!("isoloom_services_{}", at.network));
    let peering = ResourceRef::new("google_service_networking_connection", at.network);
    if m.render().is_ok_and(|t| t.contains(&format!("\"{}\"", peering.name()))) {
        return peering;
    }
    m.commented(
        &format!("Private services access on network `{}`: Cloud SQL's private addresses.", at.network),
        project(range.block())
            .add_attribute(("name", named(&format!("{}-services", at.network))))
            .add_attribute(("purpose", string("VPC_PEERING")))
            .add_attribute(("address_type", string("INTERNAL")))
            .add_attribute(("prefix_length", number(24)))
            .add_attribute(("network", reference(&net)))
            .build(),
    );
    m.block(
        peering
            .block()
            .add_attribute(("network", reference(&net)))
            .add_attribute(("service", string("servicenetworking.googleapis.com")))
            .add_attribute(("reserved_peering_ranges", list([range.attr("name")])))
            .add_attribute(("deletion_policy", string("ABANDON")))
            .add_attribute(("depends_on", list([reference("google_project_service.servicenetworking")])))
            .build(),
    );
    peering
}

/// A Cloud SQL instance at a private address, its database and user, and its name in the zone.
fn postgres(at: &Each<'_>, p: &Postgres, m: &mut Module) {
    let peering = private_access(at, m);
    let instance = ResourceRef::new("google_sql_database_instance", at.name);
    let ip = Block::builder("ip_configuration")
        .add_attribute(("ipv4_enabled", boolean(false)))
        .add_attribute(("private_network", reference(&format!("google_compute_network.{}.id", ident(at.network)))))
        .build();
    m.commented(
        &format!("Service `{}`: PostgreSQL {} on Cloud SQL, reached as {}.", at.name, p.version, at.host),
        project(instance.block())
            .add_attribute(("name", named(at.name)))
            .add_attribute(("region", reference("var.region")))
            .add_attribute(("database_version", string(format!("POSTGRES_{}", p.version))))
            .add_attribute(("deletion_protection", boolean(false)))
            .add_block(
                Block::builder("settings")
                    .add_attribute(("tier", string(tier(p.size).2)))
                    .add_attribute(("edition", string("ENTERPRISE")))
                    .add_block(ip)
                    .build(),
            )
            .add_attribute(("depends_on", list([peering.whole(), reference("google_project_service.sqladmin")])))
            .build(),
    );
    m.block(
        project(ResourceRef::new("google_sql_database", at.name).block())
            .add_attribute(("name", string(p.database.as_str())))
            .add_attribute(("instance", instance.attr("name")))
            .build(),
    );
    m.block(
        project(ResourceRef::new("google_sql_user", at.name).block())
            .add_attribute(("name", string(p.user.as_str())))
            .add_attribute(("instance", instance.attr("name")))
            .add_attribute(("password", secret(&p.password)))
            .build(),
    );
    record(at, instance.attr("private_ip_address"), m);
}

/// The service's name in the private zone, an A record at `address`.
fn record(at: &Each<'_>, address: Expression, m: &mut Module) {
    m.block(
        project(ResourceRef::new("google_dns_record_set", &ident(&at.host)).block())
            .add_attribute(("managed_zone", reference("google_dns_managed_zone.private.name")))
            .add_attribute(("name", string(format!("{}.", at.host))))
            .add_attribute(("type", string("A")))
            .add_attribute(("ttl", number(crate::cloud::vm::private_dns::TTL)))
            .add_attribute(("rrdatas", list([address])))
            .build(),
    );
}

/// A Cloud Storage bucket, never public, emptied when the environment goes; its facts as outputs.
fn object_storage(at: &Each<'_>, o: &ObjectStorage, m: &mut Module) {
    let bucket = ResourceRef::new("google_storage_bucket", at.name);
    let mut b = project(bucket.block())
        .add_attribute(("name", bucket_name(&o.bucket)))
        .add_attribute(("location", reference("var.region")))
        .add_attribute(("force_destroy", boolean(true)))
        .add_attribute(("uniform_bucket_level_access", boolean(true)))
        .add_attribute(("public_access_prevention", string("enforced")));
    if o.versioning {
        b = b.add_block(Block::builder("versioning").add_attribute(("enabled", boolean(true))).build());
    }
    m.commented(
        &format!("Service `{}`: the bucket `{}` on Cloud Storage.", at.name, o.bucket),
        b.add_attribute(("depends_on", list([reference("google_project_service.storage")]))).build(),
    );
    m.block(output(&format!("{}_bucket", ident(at.name)), bucket.attr("name")));
    let endpoint = Template::text("https://storage.googleapis.com");
    m.block(output(&format!("{}_endpoint", ident(at.name)), endpoint.into_expression()));
}
