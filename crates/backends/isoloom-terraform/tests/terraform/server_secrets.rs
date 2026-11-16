//! A launch token given as a file (supplier-portal-api's shape) on a server: Proxmox and ESXi
//! take it from a sensitive module variable of its name, written by cloud-init (`write_files`:
//! Proxmox's snippet, ESXi's guestinfo user data), and the `Secrets` step puts it in place. Its
//! value is in no generated file.

use crate::testing::{Outcome, file, plan};
use isoloom_render::backend::Backend;
use isoloom_spec::model::Target;
use isoloom_terraform::EsxiBackend;

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

/// The variable block of `name` in `tf`.
fn variable<'a>(tf: &'a str, name: &str) -> Outcome<&'a str> {
    let (_, rest) = tf
        .split_once(&format!("variable \"{name}\" {{"))
        .ok_or_else(|| format!("no variable {name}: {tf}"))?;
    Ok(rest.split_once("\n}\n").map_or(rest, |(v, _)| v))
}

#[test]
fn proxmox_writes_the_token_from_a_sensitive_variable_into_the_vms_cloud_init() -> Outcome {
    let tf = crate::testing::proxmox(SPEC)?;
    assert!(variable(&tf, "LAUNCH_TOKEN")?.contains("sensitive   = true"), "{tf}");
    assert!(tf.contains("/var/lib/isoloom/secrets.env") && tf.contains("var.LAUNCH_TOKEN"), "{tf}");
    assert!(
        tf.contains(". /var/lib/isoloom/secrets.env && set +a && rm -f /var/lib/isoloom/secrets.env"),
        "{tf}"
    );
    Ok(())
}

#[test]
fn esxi_writes_the_token_from_a_sensitive_variable_into_the_vms_guestinfo_user_data() -> Outcome {
    let files = EsxiBackend.render(&plan(SPEC, Target::Esxi)?)?;
    let tf = file(&files, "esxi/main.tf")?;
    assert!(variable(tf, "LAUNCH_TOKEN")?.contains("sensitive   = true"), "{tf}");
    assert!(tf.contains("/var/lib/isoloom/secrets.env") && tf.contains("var.LAUNCH_TOKEN"), "{tf}");
    Ok(())
}
