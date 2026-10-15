//! The target a command works on, named by its id.

use isoloom_spec::Target;

use crate::error::{CliError, Result};

/// The target with this id (`docker`, `cloud-vm`), or an error naming it.
pub(crate) fn by_id(id: &str) -> Result<Target> {
    Target::ALL
        .into_iter()
        .find(|t| t.id() == id)
        .ok_or_else(|| CliError::UnknownTarget(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_found_by_its_id_and_an_unknown_one_named() {
        assert!(matches!(by_id("cloud-vm"), Ok(Target::CloudVm)));
        assert!(matches!(by_id("bogus"), Err(CliError::UnknownTarget(id)) if id == "bogus"));
    }
}
