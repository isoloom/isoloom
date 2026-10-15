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

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    #[test]
    fn the_groups_problems_are_the_specs() {
        let reserved = lab("machines: { web: { networks: { lan: 10 }, docker: { image: nginx } } }\ngroups: { linux: { members: [web] } }\n");
        assert_eq!(problems(&reserved), ["groups.linux: Isoloom fills this group itself; choose another name"]);
    }
}
