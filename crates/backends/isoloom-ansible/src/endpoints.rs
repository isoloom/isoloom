//! `machines.json`: each existing host's SSH endpoint, for `isoloom run`, `test`, `connect` and
//! `exec`.

use isoloom_plan::Environment;
use serde_json::{Map, Value, json};

/// The user a host is reached as when the spec names none.
const DEFAULT_USER: &str = "root";
/// The SSH port a host answers on when the spec gives none.
const DEFAULT_PORT: u16 = 22;

/// Each host with an `external` address: its address, user, port and key, the defaults filled.
pub(crate) fn json(env: &Environment) -> String {
    let machines: Map<String, Value> = env
        .machines
        .iter()
        .filter_map(|(name, m)| {
            let e = m.external.as_ref()?;
            let endpoint = json!({
                "address": e.address,
                "user": e.user.as_deref().unwrap_or(DEFAULT_USER),
                "port": e.port.unwrap_or(DEFAULT_PORT),
                "key": e.key,
            });
            Some((name.clone(), endpoint))
        })
        .collect();
    format!("{:#}\n", Value::Object(machines))
}

#[cfg(test)]
mod tests {
    use crate::testing::environment;

    #[test]
    fn the_ssh_defaults_are_filled_and_an_unset_key_is_null() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12 }\noverrides: { external: { machines: { a: { ssh: { address: h.example, port: 2200 } } } } }\n",
        );
        assert_eq!(
            super::json(&env),
            "{\n  \"a\": {\n    \"address\": \"h.example\",\n    \"user\": \"root\",\n    \"port\": 2200,\n    \"key\": null\n  }\n}\n"
        );
    }

    #[test]
    fn a_machine_without_an_external_address_is_left_out() {
        let env = environment("  a: { networks: { lab: 5 }, os: debian@12 }\n");
        assert_eq!(super::json(&env), "{}\n");
    }
}
