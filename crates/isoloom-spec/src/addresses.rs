//! Several addresses on one network: a machine written `networks: { lan: [10, 11, 12] }` has
//! `.10` (its primary address, the one its name resolves to) and `.11` and `.12` besides, on the
//! same interface. A web server with a site per address, a host a lab scans at several
//! addresses, a service that moves between machines at its own address.
//!
//! The list is split on the YAML before the spec is parsed: the first number stays the
//! machine's address on the network (what every reader of `networks` already knows), and the
//! others go to [`Machine::more_addresses`](crate::model::Machine::more_addresses), which only
//! the readers that place, name or check them read.

use serde_yaml_ng::{Mapping, Value};

/// The parsed field that keeps the addresses after the first, by network.
pub const FIELD: &str = "more_addresses";

fn key(s: &str) -> Value {
    Value::String(s.to_string())
}

/// Splits every machine's address lists in place: the first stays in `networks`, the others
/// go to [`FIELD`].
pub fn split(doc: &mut Value) -> Result<(), String> {
    let Value::Mapping(top) = doc else { return Ok(()) };
    let Some(Value::Mapping(machines)) = top.get_mut(key("machines")) else {
        return Ok(());
    };
    for (name, m) in machines.iter_mut() {
        let (Some(name), Value::Mapping(m)) = (name.as_str(), m) else { continue };
        split_machine(name, m)?;
    }
    Ok(())
}

/// One machine's lists.
fn split_machine(name: &str, m: &mut Mapping) -> Result<(), String> {
    if m.contains_key(key(FIELD)) {
        return Err(format!(
            "machines.{name}.{FIELD}: not a field of the spec; write a machine's addresses on a network as a list (`lan: [10, 11]`, the first its primary one)"
        ));
    }
    let Some(Value::Mapping(nets)) = m.get_mut(key("networks")) else {
        return Ok(());
    };
    let mut more = Mapping::new();
    for (net, value) in nets.iter_mut() {
        let Value::Sequence(list) = value else { continue };
        let at = format!("machines.{name}.networks.{}", net.as_str().unwrap_or_default());
        if list.is_empty() {
            return Err(format!(
                "{at}: an empty list; give the machine's address on the network (`10`), or several (`[10, 11]`)"
            ));
        }
        if list.iter().any(|o| o.as_u64().is_none()) {
            return Err(format!(
                "{at}: each address is a number, the last octet (`[10, 11]`, the first the primary one)"
            ));
        }
        let first = list.remove(0);
        if !list.is_empty() {
            more.insert(net.clone(), Value::Sequence(std::mem::take(list)));
        }
        *value = first;
    }
    if !more.is_empty() {
        m.insert(key(FIELD), Value::Mapping(more));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    fn spec(machines: &str) -> Result<crate::Spec, crate::LoadError> {
        crate::parse(&format!(
            "version: 1\nname: t\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n{machines}"
        ))
    }

    #[test]
    fn the_first_address_is_the_primary_one() {
        let s = spec("  web: { networks: { lan: [10, 11, 12] }, vm: { os: debian@12 } }\n  db: { networks: { lan: [20] }, vm: { os: debian@12 } }\n").unwrap();
        assert_eq!(s.machines["web"].networks["lan"], 10);
        assert_eq!(s.machines["web"].more_addresses["lan"], vec![11, 12]);
        assert_eq!(s.machines["db"].networks["lan"], 20);
        assert!(s.machines["db"].more_addresses.is_empty());
    }

    #[test]
    fn a_list_is_numbers_and_the_field_is_not_written() {
        for (m, why) in [
            ("  web: { networks: { lan: [] } }\n", "an empty list"),
            ("  web: { networks: { lan: [10, x] } }\n", "each address is a number"),
            ("  web: { networks: { lan: 10 }, more_addresses: { lan: [11] } }\n", "not a field of the spec"),
        ] {
            let e = spec(m).unwrap_err().to_string();
            assert!(e.contains(why), "{e}");
        }
    }

    #[test]
    fn validation_rejects_duplicates_and_addresses_outside_the_network() {
        let s =
            spec("  web: { networks: { lan: [10, 11, 10, 255, 20] }, docker: { image: nginx } }\n  db: { networks: { lan: 20 }, docker: { image: nginx } }\n")
                .unwrap();
        let problems: Vec<String> = crate::validate(&s).into_iter().map(|p| format!("{}: {}", p.at, p.message)).collect();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("machines.web.networks.lan: address .10 is written twice"), "{problems:?}");
        assert!(problems[1].contains("255 isn't a usable address in 10.0.0.0/24"), "{problems:?}");
        assert!(
            problems[2].contains("machines.db.networks.lan: address .20 is already used by `web`"),
            "{problems:?}"
        );
    }

    #[test]
    fn clones_shift_every_address() {
        let s = spec("  ws: { count: 2, networks: { lan: [10, 20] }, vm: { os: debian@12 } }\n").unwrap();
        assert_eq!(s.machines["ws-02"].networks["lan"], 11);
        assert_eq!(s.machines["ws-02"].more_addresses["lan"], vec![21]);
    }
}
