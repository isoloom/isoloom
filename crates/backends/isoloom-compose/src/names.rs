//! The names of the services this backend adds beside the machines', each built in one place so
//! the run manifest tells them apart by the same rule.

use isoloom_plan::checks::Position;
use isoloom_spec::model::Arch;

/// Stands in for an access machine the runner supplies, so checks run from its side.
pub(crate) const STAND_IN: &str = "isoloom-access";

/// The one-shot service that copies the probe for machines of this architecture.
pub(crate) fn probe_service(arch: Arch) -> String {
    format!("isoloom-probe-{}", arch.id())
}

/// The network sidecar of a machine (or of the stand-in), which sets its routes.
pub(crate) fn routes_service(machine: &str) -> String {
    format!("{machine}-routes")
}

/// The check runner service of a position: `isoloom-check` where a user stands (`default`),
/// `isoloom-check-<position>` for the others.
pub(crate) fn check_runner_service(pos: &Position, default: &Position) -> String {
    if pos == default {
        "isoloom-check".to_owned()
    } else {
        format!("isoloom-check-{}", pos.id())
    }
}

/// The service of a tool (`tools:`).
pub(crate) fn tool_service(tool: &str) -> String {
    format!("isoloom-tool-{tool}")
}

/// The tool a service is, when it is one.
pub(crate) fn tool_of(service: &str) -> Option<&str> {
    service.strip_prefix("isoloom-tool-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runner_where_a_user_stands_has_the_short_name() {
        let user = Position::Machine("user".into());
        assert_eq!(check_runner_service(&user, &user), "isoloom-check");
        assert_eq!(check_runner_service(&Position::Networks, &user), "isoloom-check-networks");
    }

    #[test]
    fn a_tool_is_found_back_from_its_service() {
        assert_eq!(tool_of(&tool_service("shell")), Some("shell"));
        assert_eq!(tool_of("web"), None);
    }
}
