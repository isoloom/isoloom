//! What the lowered spec answers about itself: whose a volume is, and a cloud environment's
//! message.

use indexmap::IndexMap;

use super::Spec;
use crate::testing::{lab, model};

/// Three machines: web and db share `data` (`shared` holds the volume's own fields), and web
/// has `logs` of its own.
fn spec(shared: &str) -> Result<Spec, String> {
    model(&lab(&format!(
        "volumes:\n  data: {{ {shared} }}\nmachines:\n  cache: {{ docker: {{ image: redis }} }}\n  web: {{ docker: {{ image: nginx }}, volumes: {{ data: /srv, logs: /var/log/app }} }}\n  db: {{ docker: {{ image: postgres }}, volumes: {{ data: /var/lib/db }} }}\n"
    )))
}

#[test]
fn a_shared_volume_keeps_its_name_and_a_machines_own_takes_the_machines() -> Result<(), String> {
    let s = spec("")?;
    assert_eq!([s.volume_key("web", "data"), s.volume_key("web", "logs")], ["data", "web-logs"]);
    Ok(())
}

#[test]
fn a_shared_volume_is_held_by_its_owner_else_its_first_user() -> Result<(), String> {
    let s = spec("")?;
    assert_eq!(s.volume_users("data"), ["web", "db"]);
    assert_eq!(s.volume_owner("data"), Some("web"));
    assert_eq!(spec("owner: db")?.volume_owner("data"), Some("db"));
    assert_eq!(s.volume_owner("logs"), None, "a machine's own volume isn't shared");
    Ok(())
}

#[test]
fn a_cloud_message_has_its_outputs_filled_in() -> Result<(), String> {
    let s = model("version: 1\nname: t\ncloud: { provider: aws, terraform: infra }\nmessage: \"Open {{ cloud.outputs.url }} ({{ cloud.outputs.other }})\"\n")?;
    let outputs: IndexMap<String, String> = [("url".to_owned(), "https://x.example".to_owned())].into_iter().collect();
    assert_eq!(s.cloud_message(&outputs).as_deref(), Some("Open https://x.example ({{ cloud.outputs.other }})"));
    Ok(())
}
