//! `targets`: the ones asked for are possible, and at least one is.

use super::Problems;
use crate::model::{Shape, Spec};
use crate::targets::{derive, missing};

/// Requested targets must be possible, and some target must be.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    let possible = derive(spec);
    if let Some(requested) = &spec.targets {
        for (i, t) in requested.iter().enumerate() {
            if !possible.contains(t) {
                let lacking = missing(spec, t.needs());
                p.add(
                    format!("targets[{i}]"),
                    format!(
                        "`{}` needs a `{}:` implementation on every machine; missing on: {}",
                        t.id(),
                        t.needs().key(),
                        lacking.join(", ")
                    ),
                );
            }
        }
    }
    if possible.is_empty() && !spec.machines.is_empty() {
        p.add(
            "machines",
            format!(
                "no target is possible: no shape is implemented by every machine (docker missing on: {}; vm missing on: {})",
                missing(spec, Shape::Docker).join(", "),
                missing(spec, Shape::Vm).join(", ")
            ),
        );
    }
}
