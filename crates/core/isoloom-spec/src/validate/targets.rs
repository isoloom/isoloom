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

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// A container, and a machine both a container and a VM.
    const MACHINES: &str =
        "machines:\n  web: { networks: { lan: 10 }, docker: { image: nginx } }\n  db: { networks: { lan: 11 }, docker: { image: x }, vm: { os: debian@12 } }\n";

    #[test]
    fn a_requested_target_is_possible() {
        assert_eq!(problems(&lab(&format!("{MACHINES}targets: [docker]\n"))), Vec::<String>::new());
        assert_eq!(
            problems(&lab(&format!("{MACHINES}targets: [docker, vagrant]\n"))),
            ["targets[1]: `vagrant` needs a `vm:` implementation on every machine; missing on: web"]
        );
    }

    #[test]
    fn some_target_is_possible() {
        let found = problems(&lab("machines:\n  web: { networks: { lan: 10 } }\n"));
        assert_eq!(
            found,
            [
                "machines.web: give the machine a form: its `os`, plus an `app:` (or nothing more, the OS alone)",
                "machines: no target is possible: no shape is implemented by every machine (docker missing on: web; vm missing on: web)"
            ]
        );
    }
}
