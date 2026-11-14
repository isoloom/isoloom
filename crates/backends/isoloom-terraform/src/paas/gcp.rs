//! The services on Google Cloud: Cloud SQL for Postgres at a private address (private services
//! access: a range of the service's VPC network peered with Google's), public too when published;
//! a Cloud Storage bucket for object storage; Managed Service for Apache Kafka, its brokers in the
//! service's subnetwork. A key client of a bucket or a cluster gets a service account's keys (an
//! HMAC key for the S3 API, the account's key otherwise); an identity client runs as a service
//! account of its own. The APIs they need are turned on in the project Isoloom makes, as `main.tf`
//! turns on Compute's.

use isoloom_spec::paas::{Access, Api, Kafka, ObjectStorage, Postgres, Service};

use super::{Each, bucket_name, named, secret, tier};
use crate::hcl_builder::{
    BinaryOperator, Block, BlockBuilder, Expression, Module, Template, attr, binary, boolean, call, conditional, index, list, number, reference, string,
};
use crate::module::{ResourceRef, ident, output};

/// Service `at` on Google Cloud.
pub(super) fn write(at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
        Service::Kafka(k) => kafka(at, k, m),
    }
    if matches!(at.service, Service::ObjectStorage(_) | Service::Kafka(_)) && at.service.clients().values().any(|c| c.access == Access::Keys) {
        keys(at, m);
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
            Service::Kafka(_) => &["managedkafka"],
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

/// The peering of network `net` with Google's services (Cloud SQL's private addresses).
pub(super) fn private_access(net: &str, m: &mut Module) {
    let network = format!("google_compute_network.{}.id", ident(net));
    let range = ResourceRef::new("google_compute_global_address", &format!("isoloom_services_{net}"));
    m.commented(
        &format!("Private services access on network `{net}`: Cloud SQL's private addresses."),
        project(range.block())
            .add_attribute(("name", named(&format!("{net}-services"))))
            .add_attribute(("purpose", string("VPC_PEERING")))
            .add_attribute(("address_type", string("INTERNAL")))
            .add_attribute(("prefix_length", number(24)))
            .add_attribute(("network", reference(&network)))
            .build(),
    );
    m.block(
        ResourceRef::new("google_service_networking_connection", net)
            .block()
            .add_attribute(("network", reference(&network)))
            .add_attribute(("service", string("servicenetworking.googleapis.com")))
            .add_attribute(("reserved_peering_ranges", list([range.attr("name")])))
            .add_attribute(("deletion_policy", string("ABANDON")))
            .add_attribute(("depends_on", list([reference("google_project_service.servicenetworking")])))
            .build(),
    );
}

/// A Cloud SQL instance at a private address (and a public one, allow-listed, when published),
/// its database and user, and its name in the zone.
fn postgres(at: &Each<'_>, p: &Postgres, m: &mut Module) {
    let instance = ResourceRef::new("google_sql_database_instance", at.name);
    let mut ip = Block::builder("ip_configuration")
        .add_attribute(("ipv4_enabled", boolean(p.publish.is_some())))
        .add_attribute(("private_network", reference(&format!("google_compute_network.{}.id", ident(at.network)))));
    if let Some(publish) = &p.publish {
        ip = ip.add_block(
            Block::builder("authorized_networks")
                .add_attribute(("name", string("isoloom-allow")))
                .add_attribute(("value", reference(&format!("var.{}", publish.allow))))
                .build(),
        );
    }
    let peering = ResourceRef::new("google_service_networking_connection", at.network);
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
                    .add_block(ip.build())
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
    if p.publish.is_some() {
        m.block(output(&format!("{}_published", ident(at.name)), instance.attr("public_ip_address")));
    }
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
    m.block(output(&format!("{}_endpoint", ident(at.name)), string("https://storage.googleapis.com")));
}

/// A Managed Service for Apache Kafka cluster in the service's subnetwork, and its topics.
fn kafka(at: &Each<'_>, k: &Kafka, m: &mut Module) {
    let cluster = ResourceRef::new("google_managed_kafka_cluster", at.name);
    let subnet = Template::text("projects/")
        .with_expr(reference("local.project"))
        .with_text("/regions/")
        .with_expr(reference("var.region"))
        .with_text("/subnetworks/")
        .with_expr(reference(&format!("google_compute_subnetwork.{}.name", ident(at.network))));
    let access = Block::builder("access_config")
        .add_block(Block::builder("network_configs").add_attribute(("subnet", subnet.into_expression())).build())
        .build();
    let vcpus = if k.size == isoloom_spec::paas::Size::Small { 3 } else { 6 };
    m.commented(
        &format!(
            "Service `{}`: Kafka on Managed Service for Apache Kafka, its brokers in network `{}`.",
            at.name, at.network
        ),
        project(cluster.block())
            .add_attribute(("cluster_id", named(at.name)))
            .add_attribute(("location", reference("var.region")))
            .add_block(
                Block::builder("capacity_config")
                    .add_attribute(("vcpu_count", number(vcpus)))
                    .add_attribute(("memory_bytes", number(vcpus * 1_073_741_824)))
                    .build(),
            )
            .add_block(Block::builder("gcp_config").add_block(access).build())
            .add_attribute(("depends_on", list([reference("google_project_service.managedkafka")])))
            .build(),
    );
    for (topic, t) in &k.topics {
        m.block(
            project(ResourceRef::new("google_managed_kafka_topic", &format!("{}_{topic}", at.name)).block())
                .add_attribute(("topic_id", string(topic.as_str())))
                .add_attribute(("cluster", cluster.attr("cluster_id")))
                .add_attribute(("location", reference("var.region")))
                .add_attribute(("partition_count", number(u64::from(t.partitions))))
                .add_attribute(("replication_factor", number(3)))
                .build(),
        );
    }
}

/// The service account whose keys a service's key clients get, and what it may do.
fn keys(at: &Each<'_>, m: &mut Module) {
    let id = format!("isoloom_service_{}", ident(at.name));
    let account = ResourceRef::new("google_service_account", &id);
    let short = Template::text("svc-")
        .with_expr(reference("terraform_data.id.output"))
        .with_text(format!("-{}", short(at.name)));
    m.block(project(account.block()).add_attribute(("account_id", short.into_expression())).build());
    grant(at, &account.attr("email"), &id, m);
    match at.service {
        Service::ObjectStorage(o) if o.api == Api::S3 => {
            m.block(
                project(ResourceRef::new("google_storage_hmac_key", &id).block())
                    .add_attribute(("service_account_email", account.attr("email")))
                    .build(),
            );
        }
        _ => m.block(
            ResourceRef::new("google_service_account_key", &id)
                .block()
                .add_attribute(("service_account_id", account.attr("name")))
                .build(),
        ),
    }
}

/// Up to 12 letters and digits of `name`, for a service account's id.
fn short(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// The role on `at` a service account (`email`) gets, its resource named after `id`.
fn grant(at: &Each<'_>, email: &Expression, id: &str, m: &mut Module) {
    let member = Template::text("serviceAccount:").with_expr(email.clone()).into_expression();
    let b = match at.service {
        Service::ObjectStorage(_) => ResourceRef::new("google_storage_bucket_iam_member", id)
            .block()
            .add_attribute(("bucket", reference(&format!("google_storage_bucket.{}.name", ident(at.name)))))
            .add_attribute(("role", string("roles/storage.objectAdmin"))),
        Service::Kafka(_) => project(ResourceRef::new("google_project_iam_member", id).block()).add_attribute(("role", string("roles/managedkafka.client"))),
        Service::Postgres(_) => return,
    };
    m.block(b.add_attribute(("member", member)).build());
}

/// What fact file `file` of `at` holds on Google Cloud for a client with `access`.
pub(super) fn fact(at: &Each<'_>, file: &str, access: Access, client: &str) -> Option<Expression> {
    let (id, svc) = (ident(at.name), format!("isoloom_service_{}", ident(at.name)));
    let s3 = matches!(at.service, Service::ObjectStorage(o) if o.api == Api::S3);
    let value = match (at.service, file) {
        (Service::ObjectStorage(_), "endpoint") => string("https://storage.googleapis.com"),
        (Service::ObjectStorage(_), "bucket") => reference(&format!("google_storage_bucket.{id}.name")),
        (Service::ObjectStorage(_), "region") => reference("var.region"),
        (Service::ObjectStorage(_), "api") => string(if s3 { "s3" } else { "gcs" }),
        (Service::ObjectStorage(_), "access-key") if s3 => reference(&format!("google_storage_hmac_key.{svc}.access_id")),
        (Service::ObjectStorage(_), "secret-key") if s3 => reference(&format!("google_storage_hmac_key.{svc}.secret")),
        (Service::ObjectStorage(_), "access-key") => reference(&format!("google_service_account.{svc}.email")),
        (Service::ObjectStorage(_), "secret-key") => call("base64decode", [reference(&format!("google_service_account_key.{svc}.private_key"))]),
        (Service::Kafka(_), "bootstrap") => reference(&format!("google_managed_kafka_cluster.{id}.bootstrap_address")),
        (Service::Kafka(_), "security-protocol") => string("SASL_SSL"),
        (Service::Kafka(_), "sasl-mechanism") => string(if access == Access::Identity { "OAUTHBEARER" } else { "PLAIN" }),
        (Service::Kafka(_), "user") => reference(&format!("google_service_account.{svc}.email")),
        (Service::Kafka(_), "password") => reference(&format!("google_service_account_key.{svc}.private_key")),
        (_, "identity") => reference(&format!("google_service_account.isoloom_client_{}.email", ident(client))),
        _ => return None,
    };
    Some(value)
}

/// Machine `machine`'s identity: a service account of its own with each of `services`' role.
pub(super) fn identity(machine: &str, services: &[&Each<'_>], m: &mut Module) {
    let id = format!("isoloom_client_{}", ident(machine));
    let account = ResourceRef::new("google_service_account", &id);
    let short = Template::text("cli-")
        .with_expr(reference("terraform_data.id.output"))
        .with_text(format!("-{}", self::short(machine)));
    m.commented(
        &format!("Machine `{machine}`'s identity (`access: identity`): the services it uses, no key."),
        project(account.block()).add_attribute(("account_id", short.into_expression())).build(),
    );
    for at in services {
        grant(at, &account.attr("email"), &format!("{id}_{}", ident(at.name)), m);
    }
}

/// The instance resource a machine is, on Google Cloud.
pub(super) const INSTANCE: &str = "google_compute_instance";

/// What machine `machine`'s instance takes to run as its identity.
pub(super) fn attach(machine: &str) -> Vec<hcl::Structure> {
    let email = reference(&format!("google_service_account.isoloom_client_{}.email", ident(machine)));
    vec![
        Block::builder("service_account")
            .add_attribute(("email", email))
            .add_attribute(("scopes", list([string("cloud-platform")])))
            .build()
            .into(),
    ]
}

/// The address Terraform reaches machine `machine` at.
pub(super) fn public_ip(machine: &str) -> Expression {
    let nic = index(reference(&format!("google_compute_instance.{}.network_interface", ident(machine))), 0);
    attr(index(attr(nic, "access_config"), 0), "nat_ip")
}
