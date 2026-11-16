//! A launch token given as a file (supplier-portal-api's shape) on a server: Proxmox and ESXi
//! take it from a sensitive module variable of its name, written by cloud-init (`write_files`:
//! Proxmox's snippet, ESXi's guestinfo user data), and the `Secrets` step puts it in place. Its
//! value is in no generated file.

use crate::testing::{Outcome, file, has_line, plan, texts, written, written_files};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::EsxiBackend;
use isoloom_test_support::{hcl_blocks, hcl_body, hcl_text};

const SPEC: &str = "version: 1
name: portal
inputs:
  LAUNCH_TOKEN: { description: \"A token issued at launch.\" }
networks:
  lan: { cidr: 10.20.1.0/24 }
machines:
  db:
    os: debian@12
    networks: { lan: 10 }
    files:
      - { path: /run/lab/launch-token, input: LAUNCH_TOKEN }
    app: { steps: [{ run: seed.sh }] }
";

/// The secrets file's content: the token, from its sensitive variable, as the `Secrets` step reads it.
const TOKEN: &str = "ISOLOOM_SECRET_INPUT_LAUNCH_TOKEN='${base64encode(var.LAUNCH_TOKEN)}'";

/// Whether variable `name` of `tf` is sensitive.
fn sensitive(tf: &str, name: &str) -> bool {
    let body = hcl_body(tf);
    hcl_blocks(&body, "variable", &[name])
        .first()
        .is_some_and(|v| hcl_text(v.body(), "sensitive") == "true")
}

#[test]
fn proxmox_writes_the_token_from_a_sensitive_variable_into_the_vms_cloud_init() -> Outcome {
    let tf = crate::testing::proxmox(SPEC)?;
    assert!(sensitive(&tf, "LAUNCH_TOKEN"));
    // db's snippet (its cloud-config) writes the secrets file from the variable, and its set-up
    // reads it once and removes it.
    let files = written_files(&tf, "proxmox_virtual_environment_file", "db");
    let secrets = written(&files, "/var/lib/isoloom/secrets.env")?;
    assert_eq!(secrets.get("content").map(String::as_str), Some(TOKEN));
    let snippet = texts(&tf, "proxmox_virtual_environment_file", "db");
    assert!(
        has_line(&snippet, ". /var/lib/isoloom/secrets.env && set +a && rm -f /var/lib/isoloom/secrets.env"),
        "{snippet:?}"
    );
    Ok(())
}

#[test]
fn esxi_writes_the_token_from_a_sensitive_variable_into_the_vms_guestinfo_user_data() -> Outcome {
    let files = EsxiBackend.render(&plan(SPEC, Target::Esxi)?)?;
    let tf = file(&files, "esxi/main.tf")?;
    assert!(sensitive(tf, "LAUNCH_TOKEN"));
    let written_by_db = written_files(tf, "esxi_guest", "db");
    let secrets = written(&written_by_db, "/var/lib/isoloom/secrets.env")?;
    assert_eq!(secrets.get("content").map(String::as_str), Some(TOKEN));
    Ok(())
}
