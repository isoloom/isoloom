//! A machine's ports, named: Isoloom keys them by name (`machines.web.ports.http.published`), so
//! each drawn port gets a stable one. A well-known port takes its service's name (`ssh`, `http`),
//! any other `port-<n>`; a UDP port adds `-udp` (a TCP and a UDP port may share a number, never a
//! name). A name already taken on the machine gets `-2`, `-3`, ... so Isoloom still sees both and
//! judges them.

use indexmap::IndexMap;
use isoloom_spec::syntax::{PortName, PortProtocol};

use crate::document::PortDoc;
use crate::{Error, Result, Service};

/// Well-known ports, by the name their service goes by.
const WELL_KNOWN: &[(u16, &str)] = &[
    (21, "ftp"),
    (22, "ssh"),
    (23, "telnet"),
    (25, "smtp"),
    (53, "dns"),
    (69, "tftp"),
    (80, "http"),
    (88, "kerberos"),
    (110, "pop3"),
    (123, "ntp"),
    (139, "netbios"),
    (143, "imap"),
    (161, "snmp"),
    (389, "ldap"),
    (443, "https"),
    (445, "smb"),
    (636, "ldaps"),
    (1433, "mssql"),
    (1521, "oracle"),
    (2049, "nfs"),
    (3306, "mysql"),
    (3389, "rdp"),
    (5432, "postgres"),
    (5900, "vnc"),
    (5985, "winrm"),
    (6379, "redis"),
    (8080, "http-alt"),
    (8443, "https-alt"),
    (9200, "elasticsearch"),
    (27017, "mongodb"),
];

/// The name a port's service goes by, before the machine's other ports are seen.
fn base_name(s: Service) -> String {
    let port = s.port.get();
    let base = WELL_KNOWN
        .iter()
        .find(|(p, _)| *p == port)
        .map_or_else(|| format!("port-{port}"), |(_, name)| (*name).to_owned());
    if s.protocol == PortProtocol::Udp { format!("{base}-udp") } else { base }
}

/// The port as written: its number alone when it is plain TCP.
fn doc(s: Service) -> PortDoc {
    if s.protocol == PortProtocol::Tcp {
        PortDoc::Tcp(s.port)
    } else {
        PortDoc::Speaking {
            port: s.port,
            protocol: s.protocol,
        }
    }
}

/// Machine `node`'s ports, each once, by name.
///
/// # Errors
///
/// [`Error::PortName`] when a made name breaks Isoloom's rule (it never should).
pub(crate) fn named(node: &str, services: &[Service]) -> Result<IndexMap<PortName, PortDoc>> {
    let mut ports: IndexMap<PortName, PortDoc> = IndexMap::new();
    let mut seen: Vec<Service> = Vec::new();
    for &s in services {
        if seen.contains(&s) {
            continue;
        }
        seen.push(s);
        let base = base_name(s);
        // The base name, else the first `<base>-<n>` no port of the machine has taken.
        let (mut free, mut n) = (base.clone(), 1);
        while ports.keys().any(|k| k.as_str() == free) {
            n += 1;
            free = format!("{base}-{n}");
        }
        let name: PortName = free.parse().map_err(|source| Error::PortName {
            node: node.to_owned(),
            port: s.port.get(),
            source,
        })?;
        ports.insert(name, doc(s));
    }
    Ok(ports)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;

    fn service(port: u16, protocol: PortProtocol) -> Service {
        Service {
            port: NonZeroU16::new(port).unwrap(),
            protocol,
        }
    }

    fn names(services: &[Service]) -> Vec<String> {
        named("web", services).unwrap().keys().map(ToString::to_string).collect()
    }

    #[test]
    fn a_well_known_port_takes_its_services_name() {
        assert_eq!(names(&[service(22, PortProtocol::Tcp), service(8000, PortProtocol::Tcp)]), ["ssh", "port-8000"]);
    }

    #[test]
    fn a_udp_port_shares_a_number_but_not_a_name() {
        assert_eq!(names(&[service(53, PortProtocol::Tcp), service(53, PortProtocol::Udp)]), ["dns", "dns-udp"]);
    }

    #[test]
    fn a_taken_name_is_numbered_and_a_repeat_is_kept_once() {
        let s = [service(80, PortProtocol::Tcp), service(80, PortProtocol::Http), service(80, PortProtocol::Tcp)];
        assert_eq!(names(&s), ["http", "http-2"]);
    }

    #[test]
    fn plain_tcp_is_written_as_its_number() {
        assert_eq!(doc(service(80, PortProtocol::Tcp)), PortDoc::Tcp(NonZeroU16::new(80).unwrap()));
        assert!(matches!(doc(service(443, PortProtocol::Https)), PortDoc::Speaking { .. }));
    }

    proptest::proptest! {
        #[test]
        fn every_port_gets_a_name_isoloom_takes(ports in proptest::collection::vec((1u16.., 0usize..5), 0..12)) {
            let protocols = [PortProtocol::Tcp, PortProtocol::Udp, PortProtocol::Http, PortProtocol::Https, PortProtocol::Tls];
            let services: Vec<Service> = ports.iter().map(|(p, i)| service(*p, protocols[*i])).collect();
            let named = named("m", &services).unwrap();
            let unique: std::collections::HashSet<Service> = services.iter().copied().collect();
            proptest::prop_assert_eq!(named.len(), unique.len());
        }
    }
}
