//! The images of the services Isoloom adds that are this backend's own (the probe, check runner
//! and utility images are every container target's: `isoloom_render::container`).

/// The `shell` tool's image: a toolbox with tcpdump, nmap, curl, dig, netcat and more.
pub(crate) const TOOLBOX_IMAGE: &str = "nicolaka/netshoot";
/// The sidecar image of a machine on a network with `tc`, and the VLAN switches': one with `tc`
/// (the utility image has none, and an offline machine couldn't install it).
pub(crate) const TC_IMAGE: &str = "nicolaka/netshoot:v0.13";
