//! A machine's published ports as a cloud's firewall opens them: on that machine only, never
//! on its neighbors.

use isoloom_plan::MachinePlan;
use isoloom_spec::model::Protocol;

/// The ports one machine publishes, as (protocol, port): TCP first, each sorted and once.
pub(crate) fn ports(mp: &MachinePlan) -> Vec<(Protocol, u16)> {
    let mut ports: Vec<(bool, u16, Protocol)> = mp.ports.iter().map(|p| (!p.protocol.is_tcp(), p.publish, p.protocol)).collect();
    ports.sort_unstable_by_key(|(udp, port, _)| (*udp, *port));
    ports.dedup_by_key(|(udp, port, _)| (*udp, *port));
    ports.into_iter().map(|(_, port, protocol)| (protocol, port)).collect()
}
