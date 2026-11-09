//! The names of the services this backend adds beside the machines', each built in one place so
//! the run manifest tells them apart by the same rule.

use isoloom_spec::model::Arch;

pub(crate) use isoloom_render::container::{STAND_IN, check_runner as check_runner_service, routes_service};

/// The one-shot service that copies the probe for machines of this architecture.
pub(crate) fn probe_service(arch: Arch) -> String {
    format!("isoloom-probe-{}", arch.id())
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
    use isoloom_plan::checks::Position;

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
