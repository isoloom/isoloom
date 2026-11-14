//! The services on AWS: RDS for Postgres, S3 for object storage, MSK for Kafka (the databases and
//! brokers in two small subnets of a block added to the VPC, as both want two availability
//! zones); a CNAME in the private zone for each database. A bucket's keys are an IAM user's, a
//! broker's SCRAM login the spec's (kept in Secrets Manager under MSK's own prefix); a client
//! with `access: identity` gets an instance profile instead, its role allowed on each service.

use isoloom_plan::Environment;
use isoloom_spec::paas::{Access, Kafka, ObjectStorage, Postgres, Service};

use super::{Each, bucket_name, client_blocks, named, secret, tier};
use crate::hcl_builder::{Block, Expression, Module, Template, boolean, call, index, list, number, object, reference, string};
use crate::module::{ResourceRef, ident, output};

/// The block added to the VPC for the services' subnets: outside every private range a spec's
/// network takes, and one AWS adds to any VPC.
pub(crate) const SERVICES_BLOCK: &str = "100.64.0.0/26";

/// The services' two subnets, each in its own availability zone.
const SUBNETS: [(&str, &str, u64); 2] = [("isoloom_services_a", "100.64.0.0/27", 0), ("isoloom_services_b", "100.64.0.32/27", 1)];

/// The MSK version the brokers run.
const MSK_VERSION: &str = "3.8.x";

/// Service `at` on AWS.
pub(super) fn write(env: &Environment, at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(env, at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
        Service::Kafka(k) => kafka(env, at, k, m),
    }
}

/// Whether the services need the subnets of their own (a database or a broker).
pub(super) fn needs_subnets(services: &[&Service]) -> bool {
    services.iter().any(|s| matches!(s, Service::Postgres(_) | Service::Kafka(_)))
}

/// The block and subnets the databases and brokers share; routed to the internet when a
/// database is published.
pub(super) fn subnets(published: bool, m: &mut Module) {
    let block = ResourceRef::new("aws_vpc_ipv4_cidr_block_association", "isoloom_services");
    m.commented(
        "The services' own block in the VPC: RDS and MSK take subnets in two availability zones.",
        block
            .block()
            .add_attribute(("vpc_id", reference("aws_vpc.env.id")))
            .add_attribute(("cidr_block", string(SERVICES_BLOCK)))
            .build(),
    );
    for (name, cidr, zone) in SUBNETS {
        let subnet = ResourceRef::new("aws_subnet", name);
        m.block(
            subnet
                .block()
                .add_attribute(("vpc_id", block.attr("vpc_id")))
                .add_attribute(("cidr_block", string(cidr)))
                .add_attribute(("availability_zone", index(reference("data.aws_availability_zones.available.names"), zone)))
                .build(),
        );
        if published {
            m.block(
                ResourceRef::new("aws_route_table_association", name)
                    .block()
                    .add_attribute(("subnet_id", subnet.attr("id")))
                    .add_attribute(("route_table_id", reference("aws_route_table.env.id")))
                    .build(),
            );
        }
    }
}

/// The two subnets' ids.
fn subnet_ids() -> Expression {
    list(SUBNETS.map(|(n, _, _)| ResourceRef::new("aws_subnet", n).attr("id")))
}

/// A security group letting `ports` in from the networks of the service's clients (and from the
/// allow-list when it is published).
fn security_group(env: &Environment, at: &Each<'_>, ports: &[u64], m: &mut Module) -> ResourceRef {
    let sg = ResourceRef::new("aws_security_group", &format!("isoloom_service_{}", at.name));
    let mut b = sg
        .block()
        .add_attribute(("name", named(&format!("service-{}", at.name))))
        .add_attribute(("vpc_id", reference("aws_vpc.env.id")));
    for port in ports {
        let mut from: Vec<Expression> = client_blocks(env, at).into_iter().map(string).collect();
        if let Some(p) = at.service.publish() {
            from.push(reference(&format!("var.{}", p.allow)));
        }
        b = b.add_block(
            Block::builder("ingress")
                .add_attribute(("description", string("from the networks of its clients")))
                .add_attribute(("from_port", number(*port)))
                .add_attribute(("to_port", number(*port)))
                .add_attribute(("protocol", string("tcp")))
                .add_attribute(("cidr_blocks", list(from)))
                .build(),
        );
    }
    m.block(b.build());
    sg
}

/// An RDS instance, its subnet group and security group, and its name in the zone.
fn postgres(env: &Environment, at: &Each<'_>, p: &Postgres, m: &mut Module) {
    let group = ResourceRef::new("aws_db_subnet_group", at.name);
    m.commented(
        &format!("Service `{}`: PostgreSQL {} on RDS, reached as {}.", at.name, p.version, at.host),
        group
            .block()
            .add_attribute(("name", named(at.name)))
            .add_attribute(("subnet_ids", subnet_ids()))
            .build(),
    );
    let sg = security_group(env, at, &[5432], m);
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
            .add_attribute(("publicly_accessible", boolean(p.publish.is_some())))
            .add_attribute(("skip_final_snapshot", boolean(true)))
            .add_attribute(("deletion_protection", boolean(false)))
            .add_attribute(("apply_immediately", boolean(true)))
            .build(),
    );
    record(at, &db.attr("address"), m);
    if p.publish.is_some() {
        m.block(output(&format!("{}_published", ident(at.name)), db.attr("address")));
    }
}

/// The service's name in the private zone, a CNAME to `target`.
fn record(at: &Each<'_>, target: &Expression, m: &mut Module) {
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

/// A policy document allowing `actions` on `resources`.
fn policy(actions: &[&str], resources: Vec<Expression>) -> Expression {
    call(
        "jsonencode",
        [object([
            ("Version", string("2012-10-17")),
            (
                "Statement",
                list([object([
                    ("Effect", string("Allow")),
                    ("Action", list(actions.iter().map(|a| string(*a)))),
                    ("Resource", list(resources)),
                ])]),
            ),
        ])],
    )
}

/// The S3 actions a client takes on bucket `bucket`, and the resources they apply to.
fn bucket_grant(bucket: &ResourceRef) -> Expression {
    let objects = Template::new().with_expr(bucket.attr("arn")).with_text("/*").into_expression();
    policy(
        &["s3:ListBucket", "s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
        vec![bucket.attr("arn"), objects],
    )
}

/// An S3 bucket, never public, emptied when the environment goes; the IAM user whose keys its
/// key clients get.
fn object_storage(at: &Each<'_>, o: &ObjectStorage, m: &mut Module) {
    let bucket = ResourceRef::new("aws_s3_bucket", at.name);
    m.commented(
        &format!("Service `{}`: the bucket `{}` on S3.", at.name, o.bucket),
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
    let user = ResourceRef::new("aws_iam_user", &format!("isoloom_service_{}", at.name));
    m.block(
        user.block()
            .add_attribute(("name", named(&format!("service-{}", at.name))))
            .add_attribute(("force_destroy", boolean(true)))
            .build(),
    );
    m.block(
        ResourceRef::new("aws_iam_user_policy", &format!("isoloom_service_{}", at.name))
            .block()
            .add_attribute(("user", user.attr("name")))
            .add_attribute(("policy", bucket_grant(&bucket)))
            .build(),
    );
    m.block(
        ResourceRef::new("aws_iam_access_key", &format!("isoloom_service_{}", at.name))
            .block()
            .add_attribute(("user", user.attr("name")))
            .build(),
    );
    m.block(output(&format!("{}_bucket", ident(at.name)), bucket.attr("bucket")));
    m.block(output(&format!("{}_endpoint", ident(at.name)), endpoint()));
}

/// The region's S3 endpoint.
fn endpoint() -> Expression {
    Template::text("https://s3.")
        .with_expr(reference("var.region"))
        .with_text(".amazonaws.com")
        .into_expression()
}

/// An MSK cluster of two brokers, its SCRAM login the spec's (in Secrets Manager, encrypted by
/// a key of its own, as MSK asks), IAM authentication too when a client uses its identity.
fn kafka(env: &Environment, at: &Each<'_>, k: &Kafka, m: &mut Module) {
    let sg = security_group(env, at, &[9096, 9098], m);
    let partitions = k.topics.values().map(|t| u64::from(t.partitions)).max().unwrap_or(1);
    let config = ResourceRef::new("aws_msk_configuration", at.name);
    m.commented(
        &format!(
            "Service `{}`: Kafka on MSK. Its topics are made at their first use, with {partitions} partitions (MSK has no topic resource).",
            at.name
        ),
        config
            .block()
            .add_attribute(("name", named(at.name)))
            .add_attribute(("kafka_versions", list([string(MSK_VERSION)])))
            .add_attribute((
                "server_properties",
                string(format!("auto.create.topics.enable=true\nnum.partitions={partitions}\n")),
            ))
            .build(),
    );
    let sasl = Block::builder("sasl")
        .add_attribute(("scram", boolean(true)))
        .add_attribute(("iam", boolean(at.service.uses_identity())))
        .build();
    let cluster = ResourceRef::new("aws_msk_cluster", at.name);
    let brokers = Block::builder("broker_node_group_info")
        .add_attribute((
            "instance_type",
            string(if k.size == isoloom_spec::paas::Size::Small {
                "kafka.t3.small"
            } else {
                "kafka.m7g.large"
            }),
        ))
        .add_attribute(("client_subnets", subnet_ids()))
        .add_attribute(("security_groups", list([sg.attr("id")])))
        .add_block(
            Block::builder("storage_info")
                .add_block(Block::builder("ebs_storage_info").add_attribute(("volume_size", number(10))).build())
                .build(),
        )
        .build();
    m.block(
        cluster
            .block()
            .add_attribute(("cluster_name", named(at.name)))
            .add_attribute(("kafka_version", string(MSK_VERSION)))
            .add_attribute(("number_of_broker_nodes", number(2)))
            .add_block(brokers)
            .add_block(Block::builder("client_authentication").add_block(sasl).build())
            .add_block(
                Block::builder("configuration_info")
                    .add_attribute(("arn", config.attr("arn")))
                    .add_attribute(("revision", config.attr("latest_revision")))
                    .build(),
            )
            .build(),
    );
    scram(at, k, &cluster, m);
}

/// The SCRAM login MSK reads from Secrets Manager, under its `AmazonMSK_` prefix.
fn scram(at: &Each<'_>, k: &Kafka, cluster: &ResourceRef, m: &mut Module) {
    let id = format!("isoloom_service_{}", at.name);
    let key = ResourceRef::new("aws_kms_key", &id);
    m.block(
        key.block()
            .add_attribute(("description", named(at.name)))
            .add_attribute(("deletion_window_in_days", number(7)))
            .build(),
    );
    let name = Template::text("AmazonMSK_")
        .with_expr(reference("local.name"))
        .with_text(format!("-{}", at.name));
    let secret_ref = ResourceRef::new("aws_secretsmanager_secret", &id);
    m.block(
        secret_ref
            .block()
            .add_attribute(("name", name.into_expression()))
            .add_attribute(("kms_key_id", key.attr("key_id")))
            .add_attribute(("recovery_window_in_days", number(0)))
            .build(),
    );
    let login = object([("username", secret(&k.credentials.user)), ("password", secret(&k.credentials.password))]);
    m.block(
        ResourceRef::new("aws_secretsmanager_secret_version", &id)
            .block()
            .add_attribute(("secret_id", secret_ref.attr("id")))
            .add_attribute(("secret_string", call("jsonencode", [login])))
            .build(),
    );
    m.block(
        ResourceRef::new("aws_msk_scram_secret_association", &id)
            .block()
            .add_attribute(("cluster_arn", cluster.attr("arn")))
            .add_attribute(("secret_arn_list", list([secret_ref.attr("arn")])))
            .add_attribute(("depends_on", list([ResourceRef::new("aws_secretsmanager_secret_version", &id).whole()])))
            .build(),
    );
}

/// What fact file `file` of `at` holds on AWS for a client with `access`.
pub(super) fn fact(at: &Each<'_>, file: &str, access: Access, client: &str) -> Option<Expression> {
    let svc = format!("isoloom_service_{}", at.name);
    let value = match (at.service, file) {
        (Service::ObjectStorage(_), "endpoint") => endpoint(),
        (Service::ObjectStorage(_), "bucket") => reference(&format!("aws_s3_bucket.{}.bucket", ident(at.name))),
        (Service::ObjectStorage(_), "region") => reference("var.region"),
        (Service::ObjectStorage(_), "api") => string("s3"),
        (Service::ObjectStorage(_), "access-key") => reference(&format!("aws_iam_access_key.{svc}.id")),
        (Service::ObjectStorage(_), "secret-key") => reference(&format!("aws_iam_access_key.{svc}.secret")),
        (Service::Kafka(_), "bootstrap") => {
            let which = if access == Access::Identity {
                "bootstrap_brokers_sasl_iam"
            } else {
                "bootstrap_brokers_sasl_scram"
            };
            reference(&format!("aws_msk_cluster.{}.{which}", ident(at.name)))
        }
        (Service::Kafka(_), "security-protocol") => string("SASL_SSL"),
        (Service::Kafka(_), "sasl-mechanism") => string(if access == Access::Identity { "AWS_MSK_IAM" } else { "SCRAM-SHA-512" }),
        (Service::Kafka(k), "user") => secret(&k.credentials.user),
        (Service::Kafka(k), "password") => secret(&k.credentials.password),
        (_, "identity") => reference(&format!("aws_iam_role.isoloom_client_{}.arn", ident(client))),
        _ => return None,
    };
    Some(value)
}

/// Machine `machine`'s identity: a role EC2 takes on, allowed on each of `services`, and its
/// instance profile.
pub(super) fn identity(machine: &str, services: &[&Each<'_>], m: &mut Module) {
    let id = format!("isoloom_client_{}", ident(machine));
    let role = ResourceRef::new("aws_iam_role", &id);
    let assume = call(
        "jsonencode",
        [object([
            ("Version", string("2012-10-17")),
            (
                "Statement",
                list([object([
                    ("Effect", string("Allow")),
                    ("Action", string("sts:AssumeRole")),
                    ("Principal", object([("Service", string("ec2.amazonaws.com"))])),
                ])]),
            ),
        ])],
    );
    m.commented(
        &format!("Machine `{machine}`'s identity (`access: identity`): the services it uses, no key."),
        role.block()
            .add_attribute(("name", named(&format!("client-{machine}"))))
            .add_attribute(("assume_role_policy", assume))
            .build(),
    );
    for at in services {
        let grant = match at.service {
            Service::ObjectStorage(_) => bucket_grant(&ResourceRef::new("aws_s3_bucket", at.name)),
            Service::Kafka(_) => {
                let cluster = ResourceRef::new("aws_msk_cluster", at.name);
                let below = |kind: &str| {
                    Template::new()
                        .with_expr(call("replace", [cluster.attr("arn"), string(":cluster/"), string(format!(":{kind}/"))]))
                        .with_text("/*")
                        .into_expression()
                };
                policy(&["kafka-cluster:*"], vec![cluster.attr("arn"), below("topic"), below("group")])
            }
            Service::Postgres(_) => continue,
        };
        m.block(
            ResourceRef::new("aws_iam_role_policy", &format!("{id}_{}", ident(at.name)))
                .block()
                .add_attribute(("role", role.attr("id")))
                .add_attribute(("policy", grant))
                .build(),
        );
    }
    m.block(
        ResourceRef::new("aws_iam_instance_profile", &id)
            .block()
            .add_attribute(("name", named(&format!("client-{machine}"))))
            .add_attribute(("role", role.attr("name")))
            .build(),
    );
}

/// The instance resource a machine is, on AWS.
pub(super) const INSTANCE: &str = "aws_instance";

/// What machine `machine`'s instance takes to have its identity.
pub(super) fn attach(machine: &str) -> Vec<hcl::Structure> {
    let profile = reference(&format!("aws_iam_instance_profile.isoloom_client_{}.name", ident(machine)));
    vec![hcl::Attribute::new("iam_instance_profile", profile).into()]
}

/// The address Terraform reaches machine `machine` at: its Elastic IP on several networks.
pub(super) fn public_ip(env: &Environment, machine: &str) -> Expression {
    let several = env.machines.get(machine).is_some_and(|mp| mp.addresses.len() > 1);
    let kind = if several { "aws_eip" } else { "aws_instance" };
    reference(&format!("{kind}.{}.public_ip", ident(machine)))
}
