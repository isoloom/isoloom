//! The target a command works on, named by its id.

use isoloom_spec::Target;

use crate::Res;
use crate::error::CliError;

/// The target with this id, or an error naming it.
pub fn by_id(id: &str) -> Res<Target> {
    Target::ALL
        .into_iter()
        .find(|t| t.id() == id)
        .ok_or_else(|| CliError::UnknownTarget(id.to_string()).into())
}
