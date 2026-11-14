//! The services on AWS: RDS for Postgres (its subnet group over two small subnets of a block
//! added to the VPC, as RDS wants two availability zones), S3 for object storage; a CNAME in the
//! private zone for each service a client reaches.

use isoloom_plan::Environment;
use isoloom_spec::paas::{ObjectStorage, Postgres, Service};

use super::{Each, bucket_name, named, secret, tier};
use crate::hcl_builder::{Block, Module, Template, boolean, index, list, number, reference, string};
use crate::module::{ResourceRef, ident, output};

/// The block added to the VPC for the services' subnets: outside every private range a spec's
/// network takes, and one AWS adds to any VPC.
pub(crate) const SERVICES_BLOCK: &str = "100.64.0.0/26";

/// The services' two subnets, each in its own availability zone.
const SUBNETS: [(&str, &str, u64); 2] = [("isoloom_services_a", "100.64.0.0/27", 0), ("isoloom_services_b", "100.64.0.32/27", 1)];

/// Service `at` on AWS.
pub(super) fn write(env: &Environment, at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(env, at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
    }
}

/// The block and subnets the databases share.
pub(super) fn subnets(m: &mut Module) {
    let block = ResourceRef::new("aws_vpc_ipv4_cidr_block_association", "isoloom_services");
    m.commented(
        "The services' own block in the VPC: RDS takes subnets in two availability zones.",
        block
            .block()
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_attribute(("cidr_block", string(SERVICES_BLOCK)))
            .build(),
    );
    for (name, cidr, zone) in SUBNETS {
        m.block(
            ResourceRef::new("aws_subnet", name)
                .block()
                .add_attribute(("vpc_id", block.attr("vpc_id")))
                .add_attribute(("cidr_block", string(cidr)))
                .add_attribute(("availability_zone", index(reference("data.aws_availability_zones.available.names"), zone)))
                .build(),
        );
    }
}

/// An RDS instance, its subnet group and security group, and its name in the zone.
fn postgres(env: &Environment, at: &Each<'_>, p: &Postgres, m: &mut Module) {
    let group = ResourceRef::new("aws_db_subnet_group", at.name);
    m.commented(
        &format!("Service `{}`: PostgreSQL {} on RDS, reached as {}.", at.name, p.version, at.host),
        group
            .block()
            .add_attribute(("name", named(at.name)))
            .add_attribute(("subnet_ids", list(SUBNETS.map(|(n, _, _)| ResourceRef::new("aws_subnet", n).attr("id")))))
            .build(),
    );
    let sg = ResourceRef::new("aws_security_group", &format!("isoloom_service_{}", at.name));
    m.block(
        sg.block()
            .add_attribute(("name", named(&format!("service-{}", at.name))))
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_block(
                Block::builder("ingress")
                    .add_attribute(("description", string("its clients' networks")))
                    .add_attribute(("from_port", number(5432)))
                    .add_attribute(("to_port", number(5432)))
                    .add_attribute(("protocol", string("tcp")))
                    .add_attribute(("cidr_blocks", list(client_blocks(env, at).into_iter().map(string))))
                    .build(),
            )
            .build(),
    );
    let db = ResourceRef::new("aws_db_instance", at.name);
    m.block(
        db.block()
            .add_attribute(("identifier", named(at.name)))
            .add_attribute(("engine", string("postgres")))
            .add_attribute(("engine_version", string(p.version.as_str())))
            .add_attribute(("instance_class", string(tier(p.size).0)))
            .add_attribute(("allocated_storage", number(20)))
            .add_attribute(("db_name", string(p.database.as_str())))
            .add_attribute(("username", string(p.user.as_str())))
            .add_attribute(("password", secret(&p.password)))
            .add_attribute(("db_subnet_group_name", group.attr("name")))
            .add_attribute(("vpc_security_group_ids", list([sg.attr("id")])))
            .add_attribute(("publicly_accessible", boolean(false)))
            .add_attribute(("skip_final_snapshot", boolean(true)))
            .add_attribute(("deletion_protection", boolean(false)))
            .add_attribute(("apply_immediately", boolean(true)))
            .build(),
    );
    record(at, &db.attr("address"), m);
}

/// The blocks of the networks the service and its clients are on, each once.
fn client_blocks(env: &Environment, at: &Each<'_>) -> Vec<String> {
    let mut nets: Vec<&str> = vec![at.network];
    for client in at.service.clients().keys() {
        if let Some(mp) = env.spec().machines.get(client) {
            nets.extend(mp.networks.keys().map(String::as_str));
        }
    }
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

/// The service's name in the private zone, a CNAME to `target`.
fn record(at: &Each<'_>, target: &crate::hcl_builder::Expression, m: &mut Module) {
    m.block(
        ResourceRef::new("aws_route53_record", &ident(&at.host))
            .block()
            .add_attribute(("zone_id", reference("aws_route53_zone.private.zone_id")))
            .add_attribute(("name", string(at.host.as_str())))
            .add_attribute(("type", string("CNAME")))
            .add_attribute(("ttl", number(crate::cloud::vm::private_dns::TTL)))
            .add_attribute(("records", list([target.clone()])))
            .build(),
    );
}

/// An S3 bucket, never public, emptied when the environment goes; its facts as outputs.
fn object_storage(at: &Each<'_>, o: &ObjectStorage, m: &mut Module) {
    let bucket = ResourceRef::new("aws_s3_bucket", at.name);
    m.commented(
        &format!("Service `{}`: the bucket `{}` on S3 (its clients speak S3).", at.name, o.bucket),
        bucket
            .block()
            .add_attribute(("bucket", bucket_name(&o.bucket)))
            .add_attribute(("force_destroy", boolean(true)))
            .build(),
    );
    let mut closed = ResourceRef::new("aws_s3_bucket_public_access_block", at.name)
        .block()
        .add_attribute(("bucket", bucket.attr("id")));
    for field in ["block_public_acls", "block_public_policy", "ignore_public_acls", "restrict_public_buckets"] {
        closed = closed.add_attribute((field, boolean(true)));
    }
    m.block(closed.build());
    if o.versioning {
        m.block(
            ResourceRef::new("aws_s3_bucket_versioning", at.name)
                .block()
                .add_attribute(("bucket", bucket.attr("id")))
                .add_block(Block::builder("versioning_configuration").add_attribute(("status", string("Enabled"))).build())
                .build(),
        );
    }
    m.block(output(&format!("{}_bucket", ident(at.name)), bucket.attr("bucket")));
    let endpoint = Template::text("https://s3.").with_expr(reference("var.region")).with_text(".amazonaws.com");
    m.block(output(&format!("{}_endpoint", ident(at.name)), endpoint.into_expression()));
}
