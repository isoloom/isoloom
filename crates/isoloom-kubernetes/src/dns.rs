//! A machine's name resolution on Kubernetes, from its `dns:`: its own servers replace the
//! cluster's (`dnsPolicy: None`); search domains and resolver options are added to the cluster's
//! resolver otherwise, so Service names still resolve; its domain becomes the pod's subdomain.
//!
//! Kubernetes names a pod under the cluster's domain, never one of its own: with `subdomain`
//! the pod's full name is `<machine>.<subdomain>.<namespace>.svc.<cluster domain>`, in its hosts
//! file (`hostname -f`), and its hostname stays the short name, as on every other target
//! (`setHostnameAsFQDN` would make it the long one).

use isoloom_spec::model::Dns;

use crate::objects::{DnsConfig, DnsOption};

/// The pod's DNS policy (`None` with servers of its own) and its resolver settings.
pub(crate) fn config(dns: Option<&Dns>) -> (Option<&'static str>, Option<DnsConfig>) {
    let Some(d) = dns.filter(|d| !d.servers.is_empty() || !d.search.is_empty() || !d.options.is_empty()) else {
        return (None, None);
    };
    let policy = (!d.servers.is_empty()).then_some("None");
    let config = DnsConfig {
        nameservers: d.servers.clone(),
        searches: d.search.clone(),
        options: d.options.iter().map(|o| DnsOption::parse(o)).collect(),
    };
    (policy, Some(config))
}

/// The pod's subdomain: its domain as one DNS label (`corp.local` is `corp-local`), when it has
/// one that makes a label.
pub(crate) fn subdomain(dns: Option<&Dns>) -> Option<String> {
    let domain = dns?.domain.as_deref()?;
    let label: String = domain
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' })
        .take(63)
        .collect();
    let label = label.trim_matches('-');
    (label.starts_with(|c: char| c.is_ascii_lowercase())).then(|| label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dns(servers: &[&str], search: &[&str], options: &[&str], domain: Option<&str>) -> Dns {
        let own = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect();
        Dns {
            servers: own(servers),
            search: own(search),
            domain: domain.map(str::to_string),
            options: own(options),
        }
    }

    #[test]
    fn own_servers_replace_the_cluster_resolver_and_the_rest_adds_to_it() {
        let (policy, resolver) = super::config(Some(&dns(&["10.0.0.2"], &[], &["ndots:2"], None)));
        assert_eq!(policy, Some("None"));
        let c = resolver.unwrap();
        assert_eq!(c.nameservers, ["10.0.0.2"]);
        assert_eq!((c.options[0].name.as_str(), c.options[0].value.as_deref()), ("ndots", Some("2")));
        // Search domains and options only: the cluster's resolver stays, with them added.
        let (policy, resolver) = super::config(Some(&dns(&[], &["corp.local"], &["rotate"], None)));
        assert_eq!(policy, None);
        let c = resolver.unwrap();
        assert_eq!(c.searches, ["corp.local"]);
        assert_eq!((c.options[0].name.as_str(), c.options[0].value.as_deref()), ("rotate", None));
        assert_eq!(super::config(Some(&dns(&[], &[], &[], Some("corp.local")))).1.map(|_| ()), None);
        assert_eq!(super::config(None).0, None);
    }

    #[test]
    fn the_domain_is_one_label() {
        assert_eq!(subdomain(Some(&dns(&[], &[], &[], Some("Corp.Local")))).as_deref(), Some("corp-local"));
        assert_eq!(subdomain(Some(&dns(&[], &[], &[], Some("arm.lab")))).as_deref(), Some("arm-lab"));
        assert_eq!(subdomain(Some(&dns(&[], &[], &[], Some("9lives.lab")))), None);
        assert_eq!(subdomain(Some(&dns(&[], &[], &[], None))), None);
        let long = "a".repeat(70);
        assert_eq!(subdomain(Some(&dns(&[], &[], &[], Some(&long)))).map(|l| l.len()), Some(63));
    }
}
