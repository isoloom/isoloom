//! The Proxmox module's SOCKS5 proxy (not run on a real host): a server whose lab bridge only its
//! node reaches has the router out of reach of the machine running Terraform, so every SSH
//! connection the module makes (the waits for the machines' set-up, the seal) may go through a
//! proxy first, a tunnel to the node; with none set, they are direct, as on every other target.

use crate::testing::{Outcome, file, plan, proxmox};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::EsxiBackend;
use isoloom_test_support::{assert_every, hcl_attribute, hcl_blocks, hcl_body, hcl_nested, hcl_one, hcl_text};

const SEALED: &str = "version: 1
name: proxied
networks:
  lab: { cidr: 10.80.0.0/24, internet: false }
machines:
  web: { networks: { lab: 10 }, os: debian@12, app: { steps: [{ run: s.sh }] } }
";

/// Each `connection` block of `terraform_data.<name>` in `tf`.
fn connections<'b>(tf: &'b hcl::Body, name: &str) -> Vec<&'b hcl::Body> {
    hcl_nested(hcl_one(tf, "terraform_data", name), "connection").iter().map(|c| c.body()).collect()
}

#[test]
fn the_waits_and_the_seal_go_through_the_proxy_only_when_one_is_set() -> Outcome {
    let tf = hcl_body(&proxmox(SEALED)?);
    for name in ["isoloom_ready_web", "isoloom_seal"] {
        let found = connections(&tf, name);
        assert_eq!(found.len(), 1, "{name}");
        let c = found[0];
        assert_eq!(hcl_text(c, "proxy_scheme"), r#"var.ssh_proxy_host != "" ? "socks5" : null"#, "{name}");
        assert_eq!(hcl_text(c, "proxy_host"), r#"var.ssh_proxy_host != "" ? var.ssh_proxy_host : null"#, "{name}");
        assert_eq!(hcl_text(c, "proxy_port"), r#"var.ssh_proxy_host != "" ? var.ssh_proxy_port : null"#, "{name}");
    }
    Ok(())
}

#[test]
fn the_proxy_is_unset_by_default() -> Outcome {
    let tf = hcl_body(&proxmox(SEALED)?);
    for (name, default) in [("ssh_proxy_host", r#""""#), ("ssh_proxy_port", "0")] {
        let variable = hcl_blocks(&tf, "variable", &[name]);
        assert_eq!(variable.len(), 1, "{name}");
        let got = hcl_attribute(variable[0].body(), "default").map(hcl::format::to_string).transpose()?;
        assert_eq!(got.as_deref(), Some(default), "{name}");
    }
    Ok(())
}

#[test]
fn other_servers_connect_directly() -> Outcome {
    let tf = hcl_body(file(&EsxiBackend.render(&plan(SEALED, Target::Esxi)?)?, "esxi/main.tf")?);
    // Its seal connects, and to the router directly: no proxy setting, no proxy variable.
    let seal = connections(&tf, "isoloom_seal");
    assert_eq!(seal.len(), 1);
    assert_every!(seal, |c: &&hcl::Body| c.attributes().all(|a| !a.key().starts_with("proxy_")));
    assert_eq!(hcl_blocks(&tf, "variable", &["ssh_proxy_host"]).len(), 0);
    Ok(())
}
