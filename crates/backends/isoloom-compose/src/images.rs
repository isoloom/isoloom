//! The images of the services Isoloom adds that are this backend's own (the probe, check runner
//! and utility images are every container target's: `isoloom_render::container`; the `shell`
//! tool's is the plan's).

/// The sidecar image of a machine on a network with `tc`, and the VLAN switches': one with `tc`
/// (the utility image has none, and an offline machine couldn't install it). The `shell` tool's
/// toolbox, pinned by digest with it.
pub(crate) const TC_IMAGE: &str = isoloom_plan::tools::SHELL_IMAGE;
