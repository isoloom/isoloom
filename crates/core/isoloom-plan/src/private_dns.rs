//! The environment's private DNS zone (`dns.private`): every machine's name and `aliases` under
//! the zone's domain, at its first address. The clouds with a DNS of their own write the zone
//! and its records; the Docker targets give the names to Docker's DNS as network aliases; every
//! VM's hosts file carries them too, so they resolve where the platform has no DNS to serve
//! them (Vagrant, Proxmox, DigitalOcean, Linode) and before a cloud's zone answers.

use std::net::Ipv4Addr;

use isoloom_spec::model::Spec;

use crate::addressing::address;
use crate::requirements::Requirement;

/// The zone, as every target serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PrivateDns {
    /// The zone's domain (`lab.internal`).
    pub domain: String,
    /// One record per name, machines in spec order, each machine's own name before its aliases.
    pub records: Vec<Record>,
}

/// One name of the zone.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    /// The full name (`web.lab.internal`).
    pub name: String,
    /// The name within the zone (`web`), as the clouds whose records are relative take it.
    pub label: String,
    /// The machine it names.
    pub machine: String,
    /// The machine's address on its first network.
    pub address: Ipv4Addr,
}

/// The zone of `spec`, when `dns.private` is on.
#[must_use]
pub fn plan(spec: &Spec) -> Option<PrivateDns> {
    let domain = spec.private_domain()?;
    let mut records = Vec::new();
    for (name, m) in &spec.machines {
        let Some((net, octet)) = m.networks.iter().next() else { continue };
        let at = address(spec, net, *octet);
        for label in std::iter::once(name).chain(&m.aliases) {
            records.push(Record {
                name: format!("{label}.{domain}"),
                label: label.clone(),
                machine: name.clone(),
                address: at,
            });
        }
    }
    Some(PrivateDns { domain, records })
}

/// `names` (a machine's own and its aliases), then each of them under the private zone's domain
/// when `dns.private` is on.
#[must_use]
pub fn with_domain(spec: &Spec, names: Vec<String>) -> Vec<String> {
    match spec.private_domain() {
        Some(domain) => {
            let full: Vec<String> = names.iter().map(|n| format!("{n}.{domain}")).collect();
            names.into_iter().chain(full).collect()
        }
        None => names,
    }
}

/// What the zone asks of a backend: names it serves itself, by its DNS or each VM's hosts file.
#[must_use]
pub fn requirements(spec: &Spec) -> Option<Requirement> {
    spec.private_domain().map(|domain| Requirement::PrivateDns { domain })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(dns: &str) -> Spec {
        crate::testing::spec(&format!(
            "version: 1\nname: lab\n{dns}\nnetworks: {{ lan: {{ cidr: 10.0.0.0/24 }} }}\nmachines:\n  web: {{ networks: {{ lan: 10 }}, aliases: [www], app: {{ image: nginx }} }}\n  db: {{ networks: {{ lan: 20 }}, app: {{ image: postgres }} }}\n  box: {{ app: {{ image: alpine }} }}\n"
        ))
    }

    #[test]
    fn every_name_and_alias_is_a_record_at_the_first_address() {
        let z = plan(&spec("dns: { private: true, domain: corp.lab }")).unwrap();
        assert_eq!(z.domain, "corp.lab");
        let records: Vec<(&str, &str, String)> = z.records.iter().map(|r| (r.name.as_str(), r.label.as_str(), r.address.to_string())).collect();
        assert_eq!(
            records,
            [
                ("web.corp.lab", "web", "10.0.0.10".to_owned()),
                ("www.corp.lab", "www", "10.0.0.10".to_owned()),
                ("db.corp.lab", "db", "10.0.0.20".to_owned()),
            ]
        );
        assert_eq!(
            requirements(&spec("dns: { private: true }")),
            Some(Requirement::PrivateDns { domain: "lab.internal".into() })
        );
    }

    #[test]
    fn off_there_is_no_zone_and_names_stay_short() {
        let s = spec("");
        assert_eq!(plan(&s), None);
        assert_eq!(requirements(&s), None);
        assert_eq!(with_domain(&s, vec!["web".into()]), ["web"]);
        let on = spec("dns: { private: true }");
        assert_eq!(
            with_domain(&on, vec!["web".into(), "www".into()]),
            ["web", "www", "web.lab.internal", "www.lab.internal"]
        );
    }
}
