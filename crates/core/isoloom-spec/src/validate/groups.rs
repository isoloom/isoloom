//! `groups` and `common`: names and members (the rules live with the groups' normalization in
//! [`crate::groups`]).

use super::Problems;
use crate::model::Spec;

/// Groups: names and members.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    for (at, message) in crate::groups::problems(spec) {
        p.add(at, message);
    }
}
