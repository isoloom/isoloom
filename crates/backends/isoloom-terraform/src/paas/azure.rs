//! The services on Azure: a PostgreSQL flexible server without public access, reached through
//! a private endpoint in the service's subnet; a storage account and its container for object
//! storage (`api: platform` only: Blob Storage has no S3 API). Each service a client reaches has
//! an A record at its private endpoint in the private zone.

use isoloom_spec::paas::{ObjectStorage, Postgres, Service};

use super::{Each, named, secret, tier};
use crate::hcl_builder::{Block, BlockBuilder, Expression, Module, Template, attr, boolean, index, list, number, reference, string};
use crate::module::{ResourceRef, ident, output};

/// Service `at` on Azure.
pub(super) fn write(at: &Each<'_>, m: &mut Module) {
    match at.service {
        Service::Postgres(p) => postgres(at, p, m),
        Service::ObjectStorage(o) => object_storage(at, o, m),
    }
}

/// The resource group, location and tags every resource takes.
fn placed(b: BlockBuilder) -> BlockBuilder {
    b.add_attribute(("resource_group_name", reference("azurerm_resource_group.env.name")))
        .add_attribute(("location", reference("var.region")))
        .add_attribute(("tags", reference("local.tags")))
}

/// A flexible server, its database, its private endpoint and its name in the zone.
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
            .add_attribute(("public_network_access_enabled", boolean(false)))
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
