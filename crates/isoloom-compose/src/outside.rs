//! Existing Docker networks (`external`): Compose joins them by name and never creates or
//! removes them, so they carry none of Isoloom's labels. A container joins one at its fixed
//! address, after its own networks.

use indexmap::IndexMap;
use isoloom_plan::{Environment, Join, MachinePlan, OutsideAddress};

use crate::file::{Attachment, Network};

/// The existing networks the environment joins, by their name on the Docker host.
pub(crate) fn networks(env: &Environment) -> IndexMap<String, Network> {
    env.outside
        .iter()
        .filter_map(|(key, n)| match &n.join {
            Join::Existing(name) => Some((
                key.clone(),
                Network {
                    external: true,
                    name: Some(name.clone()),
                    ..Network::default()
                },
            )),
            // Refused before rendering: Compose has no bridged networks.
            Join::Bridged(_) => None,
        })
        .collect()
}

/// A container's attachments to the existing networks it joins, at its fixed address.
pub(crate) fn attachments(mp: &MachinePlan) -> impl Iterator<Item = (String, Option<Attachment>)> + '_ {
    mp.outside.iter().filter_map(|(net, a)| match a {
        OutsideAddress::Fixed(ip) => Some((
            net.clone(),
            Some(Attachment {
                ipv4_address: Some(ip.to_string()),
                ..Attachment::default()
            }),
        )),
        OutsideAddress::Dhcp => None,
    })
}
