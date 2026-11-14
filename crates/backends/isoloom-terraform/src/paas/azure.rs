//! The services on Azure: a PostgreSQL flexible server reached through a private endpoint in the
//! service's subnet (or published to the allow-list); a storage account and its container for
//! object storage (`api: platform` only: Blob Storage has no S3 API); an Event Hubs namespace for
//! Kafka (its Kafka endpoint, a hub per topic). A client with `access: identity` gets a
//! user-assigned identity with the data role of each service it uses; the others get the keys
//! Azure issues (the account's key, the namespace's connection string).

use isoloom_spec::paas::{Access, Kafka, ObjectStorage, Postgres, Service};

use super::{Each, named, secret, tier};
use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, attr, boolean, call, index, list, number, reference, string};
use crate::module::{ResourceRef, ident, output};

/// Service `at` on Azure.
pub(super) fn write(at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
        Service::Kafka(k) => kafka(at, k, m),
    }
}

/// The resource group, location and tags every resource takes.
fn placed(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
        .add_attribute(("location", reference("var.region")))
        .add_attribute(("tags", reference("local.tags")))
}

/// A flexible server, its database, its private endpoint (or its allow-list) and its name in the
/// zone.
fn postgres(at: &Each<'_>, p: &Postgres, m: &mut Module) {
    let server = ResourceRef::new("azurerm_postgresql_flexible_server", at.name);
    m.commented(
        &format!("Service `{}`: PostgreSQL {} on a flexible server, reached as {}.", at.name, p.version, at.host),
        placed(server.block().add_attribute(("name", named(at.name))))
            .add_attribute(("version", string(p.version.as_str())))
            .add_attribute(("administrator_login", string(p.user.as_str())))
            .add_attribute(("administrator_password", secret(&p.password)))
            .add_attribute(("sku_name", string(tier(p.size).1)))
            .add_attribute(("storage_mb", number(32768)))
            .add_attribute(("public_network_access_enabled", boolean(p.publish.is_some())))
            .add_block(Block::builder("lifecycle").add_attribute(("ignore_changes", list([reference("zone")]))).build())
            .build(),
    );
    m.block(
        ResourceRef::new("azurerm_postgresql_flexible_server_database", at.name)
            .block()
            .add_attribute(("name", string(p.database.as_str())))
            .add_attribute(("server_id", server.attr("id")))
            .add_attribute(("charset", string("UTF8")))
            .add_attribute(("collation", string("en_US.utf8")))
            .build(),
    );
    if let Some(publish) = &p.publish {
        let allow = reference(&format!("var.{}", publish.allow));
        m.block(
            ResourceRef::new("azurerm_postgresql_flexible_server_firewall_rule", at.name)
                .block()
                .add_attribute(("name", string("isoloom-allow")))
                .add_attribute(("server_id", server.attr("id")))
                .add_attribute(("start_ip_address", call("cidrhost", [allow.clone(), number(0)])))
                .add_attribute(("end_ip_address", call("cidrhost", [allow, Expression::from(-1)])))
                .build(),
        );
        m.block(output(&format!("{}_published", ident(at.name)), server.attr("fqdn")));
    }
    let endpoint = private_endpoint(at, &server, "postgresqlServer", m);
    record(at, endpoint, m);
}

/// A private endpoint of `resource`'s `subresource` in the service's subnet: its address.
fn private_endpoint(at: &Each<'_>, resource: &ResourceRef, subresource: &str, m: &mut Module) -> Expression {
    let pe = ResourceRef::new("azurerm_private_endpoint", &format!("isoloom_service_{}", at.name));
    m.block(
        placed(pe.block().add_attribute(("name", named(&format!("service-{}", at.name)))))
            .add_attribute(("subnet_id", reference(&format!("azurerm_subnet.{}.id", ident(at.network)))))
            .add_block(
                Block::builder("private_service_connection")
                    .add_attribute(("name", named(&format!("service-{}", at.name))))
                    .add_attribute(("private_connection_resource_id", resource.attr("id")))
                    .add_attribute(("subresource_names", list([string(subresource)])))
                    .add_attribute(("is_manual_connection", boolean(false)))
                    .build(),
            )
            .build(),
    );
    attr(
        index(reference(&format!("azurerm_private_endpoint.{}.private_service_connection", pe.name())), 0),
        "private_ip_address",
    )
}

/// The service's name in the private zone, an A record at `address`.
fn record(at: &Each<'_>, address: Expression, m: &mut Module) {
    let label = at.host.split('.').next().unwrap_or(at.name);
    m.block(
        ResourceRef::new("azurerm_private_dns_a_record", &ident(&at.host))
            .block()
            .add_attribute(("name", string(label)))
            .add_attribute(("private_dns_zone_id", reference("azurerm_private_dns_zone.private.id")))
            .add_attribute(("ttl", number(crate::cloud::vm::private_dns::TTL)))
            .add_attribute(("records", list([address])))
            .add_attribute(("tags", reference("local.tags")))
            .build(),
    );
}

/// A storage account (its name unique to the copy, 24 characters at most) and the container.
fn object_storage(at: &Each<'_>, o: &ObjectStorage, m: &mut Module) {
    let account = ResourceRef::new("azurerm_storage_account", at.name);
    let name = Template::text("isoloom")
        .with_expr(reference("terraform_data.id.output"))
        .with_text(short(at.name));
    let mut b = placed(account.block().add_attribute(("name", name.into_expression())))
        .add_attribute(("account_tier", string("Standard")))
        .add_attribute(("account_replication_type", string("LRS")))
        .add_attribute(("min_tls_version", string("TLS1_2")))
        .add_attribute(("allow_nested_items_to_be_public", boolean(false)));
    if o.versioning {
        b = b.add_block(Block::builder("blob_properties").add_attribute(("versioning_enabled", boolean(true))).build());
    }
    m.commented(
        &format!(
            "Service `{}`: the container `{}` in a storage account (its clients speak Blob Storage's API).",
            at.name, o.bucket
        ),
        b.build(),
    );
    let container = ResourceRef::new("azurerm_storage_container", at.name);
    m.block(
        container
            .block()
            .add_attribute(("name", string(o.bucket.as_str())))
            .add_attribute(("storage_account_id", account.attr("id")))
            .add_attribute(("container_access_type", string("private")))
            .build(),
    );
    m.block(output(&format!("{}_bucket", ident(at.name)), container.attr("name")));
    m.block(output(&format!("{}_endpoint", ident(at.name)), account.attr("primary_blob_endpoint")));
}

/// Up to 9 letters and digits of `name`, for a storage account's name.
fn short(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).take(9).collect()
}

/// An Event Hubs namespace (its Kafka endpoint), a hub per topic, and the rule whose
/// connection string its key clients get.
fn kafka(at: &Each<'_>, k: &Kafka, m: &mut Module) {
    let ns = ResourceRef::new("azurerm_eventhub_namespace", at.name);
    m.commented(
        &format!("Service `{}`: Kafka on Event Hubs (its Kafka endpoint, port 9093), a hub per topic.", at.name),
        placed(ns.block().add_attribute(("name", named(at.name))))
            .add_attribute(("sku", string("Standard")))
            .add_attribute(("capacity", number(1)))
            .build(),
    );
    for (topic, t) in &k.topics {
        m.block(
            ResourceRef::new("azurerm_eventhub", &format!("{}_{topic}", at.name))
                .block()
                .add_attribute(("name", string(topic.as_str())))
                .add_attribute(("namespace_id", ns.attr("id")))
                .add_attribute(("partition_count", number(u64::from(t.partitions))))
                .add_attribute(("message_retention", number(1)))
                .build(),
        );
    }
    m.block(
        ResourceRef::new("azurerm_eventhub_namespace_authorization_rule", &format!("isoloom_service_{}", at.name))
            .block()
            .add_attribute(("name", string("isoloom-clients")))
            .add_attribute(("namespace_name", ns.attr("name")))
            .add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
            .add_attribute(("listen", boolean(true)))
            .add_attribute(("send", boolean(true)))
            .add_attribute(("manage", boolean(false)))
            .build(),
    );
}

/// What fact file `file` of `at` holds on Azure for a client with `access`.
pub(super) fn fact(at: &Each<'_>, file: &str, access: Access, client: &str) -> Option<Expression> {
    let id = ident(at.name);
    let value = match (at.service, file) {
        (Service::ObjectStorage(_), "endpoint") => reference(&format!("azurerm_storage_account.{id}.primary_blob_endpoint")),
        (Service::ObjectStorage(_), "bucket") => reference(&format!("azurerm_storage_container.{id}.name")),
        (Service::ObjectStorage(_), "region") => reference("var.region"),
        (Service::ObjectStorage(_), "api") => string("azure-blob"),
        (Service::ObjectStorage(_), "access-key") => reference(&format!("azurerm_storage_account.{id}.name")),
        (Service::ObjectStorage(_), "secret-key") => reference(&format!("azurerm_storage_account.{id}.primary_access_key")),
        (Service::Kafka(_), "bootstrap") => Template::new()
            .with_expr(reference(&format!("azurerm_eventhub_namespace.{id}.name")))
            .with_text(".servicebus.windows.net:9093")
            .into_expression(),
        (Service::Kafka(_), "security-protocol") => string("SASL_SSL"),
        (Service::Kafka(_), "sasl-mechanism") => string(if access == Access::Identity { "OAUTHBEARER" } else { "PLAIN" }),
        (Service::Kafka(_), "user") => string("$ConnectionString"),
        (Service::Kafka(_), "password") => reference(&format!(
            "azurerm_eventhub_namespace_authorization_rule.isoloom_service_{id}.primary_connection_string"
        )),
        (_, "identity") => reference(&format!("azurerm_user_assigned_identity.isoloom_client_{}.client_id", ident(client))),
        _ => return None,
    };
    Some(value)
}

/// Machine `machine`'s identity: a user-assigned identity with each of `services`' data role.
pub(super) fn identity(machine: &str, services: &[&Each<'_>], m: &mut Module) {
    let id = format!("isoloom_client_{}", ident(machine));
    let identity = ResourceRef::new("azurerm_user_assigned_identity", &id);
    m.commented(
        &format!("Machine `{machine}`'s identity (`access: identity`): the services it uses, no key."),
        placed(identity.block().add_attribute(("name", named(&format!("client-{machine}"))))).build(),
    );
    for at in services {
        let (role, scope) = match at.service {
            Service::ObjectStorage(_) => ("Storage Blob Data Contributor", format!("azurerm_storage_account.{}.id", ident(at.name))),
            Service::Kafka(_) => ("Azure Event Hubs Data Owner", format!("azurerm_eventhub_namespace.{}.id", ident(at.name))),
            Service::Postgres(_) => continue,
        };
        m.block(
            ResourceRef::new("azurerm_role_assignment", &format!("{id}_{}", ident(at.name)))
                .block()
                .add_attribute(("scope", reference(&scope)))
                .add_attribute(("role_definition_name", string(role)))
                .add_attribute(("principal_id", identity.attr("principal_id")))
                .build(),
        );
    }
}

/// The VM resource a Linux machine is, on Azure.
pub(super) const INSTANCE: &str = "azurerm_linux_virtual_machine";

/// What machine `machine`'s VM takes to have its identity.
pub(super) fn attach(machine: &str) -> Vec<hcl::Structure> {
    let ids = list([reference(&format!("azurerm_user_assigned_identity.isoloom_client_{}.id", ident(machine)))]);
    vec![
        Block::builder("identity")
            .add_attribute(("type", string("UserAssigned")))
            .add_attribute(("identity_ids", ids))
            .build()
            .into(),
    ]
}

/// The address Terraform reaches machine `machine` at.
pub(super) fn public_ip(machine: &str) -> Expression {
    reference(&format!("azurerm_public_ip.{}.ip_address", ident(machine)))
}
