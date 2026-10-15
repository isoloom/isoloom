//! The lowered form read from YAML written in its own shape (`docker:` and `vm:` blocks), for
//! the tests that build a lowered spec by hand. Authors write [`crate::syntax`], which the plan
//! lowers into this form.

use super::Spec;
use crate::error::{LoadError, Stage};
use crate::{addresses, clones, groups, os, vlans};

/// Reads a lowered spec from YAML in the model's own shape: clones, groups and addresses folded
/// in, each OS resolved, VLANs made networks.
///
/// # Errors
///
/// [`LoadError`]: not YAML, an unknown field, or a normalization step that refuses the document.
pub fn from_yaml(yaml: &str) -> Result<Spec, LoadError> {
    let mut doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml)?;
    // A machine's `os` reaches its `vm:` before clones copy the machine and before a shared
    // `vm:` could fill its OS.
    os::fill_vm(&mut doc);
    let clones = clones::expand(&mut doc).map_err(LoadError::at(Stage::Clones))?;
    groups::expand(&mut doc).map_err(LoadError::at(Stage::Groups))?;
    addresses::split(&mut doc).map_err(LoadError::at(Stage::Addresses))?;
    let mut spec: Spec = serde_yaml_ng::from_value(doc)?;
    spec.clones = clones;
    os::canonical(&mut spec).map_err(LoadError::at(Stage::Os))?;
    vlans::flatten(spec).map_err(LoadError::at(Stage::Vlans))
}
