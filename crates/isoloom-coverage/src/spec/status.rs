//! What an output does with a field, as the tables state it, and the constants the tables share.

/// What an output does with a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Produced; `proof` is an example that uses the field and generates for this output, or
    /// [`PROBE`] when no example does and the field's probe spec stands in for one.
    Done {
        /// What the output writes for it.
        how: &'static str,
        /// The example that shows it, or [`PROBE`].
        proof: &'static str,
    },
    /// Produced, with a known gap.
    Partial {
        /// What the output writes for it.
        how: &'static str,
        /// What it doesn't cover yet.
        gap: &'static str,
        /// The example that shows it, or [`PROBE`].
        proof: &'static str,
    },
    /// Not produced yet.
    Planned {
        /// What it comes with.
        note: &'static str,
    },
    /// Means nothing for this output (`vm:` in a Compose file).
    NotApplicable {
        /// Why the output has no use for it.
        why: &'static str,
    },
    /// Describes the environment for people and runners; no output uses it.
    Descriptive {
        /// Who reads it.
        note: &'static str,
    },
    /// Read by Isoloom itself (validation, target selection), not by an output.
    Core {
        /// What Isoloom does with it.
        note: &'static str,
    },
}

/// The proof of a claim no example exercises on that output: the field's probe spec (see
/// [`super::probes`]) generates for it, which the tests check.
pub const PROBE: &str = "(probe)";

/// A VM or existing-host output's cell for secrets: what delivering them there comes with.
pub const SECRETS_ON_VMS: Status = Status::Planned {
    note: "secrets on VMs and existing hosts: files written over the machine's connection by `isoloom run`",
};

/// A container output's cell for a field of `vm:`.
pub const NOT_A_VM: Status = Status::NotApplicable {
    why: "container outputs build machines from `docker:`",
};

/// A VM output's cell for a field of `docker:`.
pub const NOT_A_CONTAINER: Status = Status::NotApplicable {
    why: "VM outputs build machines from `vm:`",
};

/// A field naming a Vagrant box, on an output that boots images of its own.
pub const VAGRANT_BOX: Status = Status::NotApplicable {
    why: "names a Vagrant box: this output boots its own image for the OS",
};

/// A field that builds a machine, on the output that uses machines already there.
pub const EXISTING_HOST: Status = Status::NotApplicable {
    why: "an existing host is used as it is",
};

impl Status {
    /// The mark the table shows for it.
    pub fn symbol(self) -> &'static str {
        match self {
            Status::Done { .. } => "✓",
            Status::Partial { .. } => "◐",
            Status::Planned { .. } => "planned",
            Status::NotApplicable { .. } => "n/a",
            Status::Descriptive { .. } => "info",
            Status::Core { .. } => "core",
        }
    }

    /// Whether the output can be expected to handle the field (done, partial or planned).
    pub fn applies(self) -> bool {
        matches!(self, Status::Done { .. } | Status::Partial { .. } | Status::Planned { .. })
    }

    /// The example or probe a done or partial claim rests on.
    pub fn proof(self) -> Option<&'static str> {
        match self {
            Status::Done { proof, .. } | Status::Partial { proof, .. } => Some(proof),
            _ => None,
        }
    }

    /// What the table says for it: how it's produced, and the gap or the reason.
    pub fn note(self) -> String {
        match self {
            Status::Done { how, .. } => how.to_string(),
            Status::Partial { how, gap, .. } => format!("{how}; not yet: {gap}"),
            Status::Planned { note } | Status::NotApplicable { why: note } | Status::Descriptive { note } | Status::Core { note } => note.to_string(),
        }
    }

    /// How far along the output is, for combining two outputs into one: the less done wins.
    fn rank(self) -> u8 {
        match self {
            Status::Planned { .. } => 0,
            Status::Partial { .. } => 1,
            Status::Done { .. } => 2,
            Status::NotApplicable { .. } | Status::Descriptive { .. } | Status::Core { .. } => 3,
        }
    }

    /// An output made of two others (hybrid: Compose for its containers, Vagrant for its VMs):
    /// a field one of them has no use for is the other's; otherwise the less done of the two.
    #[must_use]
    pub fn combine(self, other: Status) -> Status {
        if self.rank() <= other.rank() { self } else { other }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DONE: Status = Status::Done { how: "a", proof: "x" };
    const PARTIAL: Status = Status::Partial {
        how: "b",
        gap: "c",
        proof: "x",
    };
    const PLANNED: Status = Status::Planned { note: "d" };

    #[test]
    fn every_status_has_a_mark_and_a_note() {
        let all = [DONE, PARTIAL, PLANNED, NOT_A_VM, Status::Descriptive { note: "e" }, Status::Core { note: "f" }];
        let marks: Vec<_> = all.iter().map(|s| s.symbol()).collect();
        assert_eq!(marks, ["✓", "◐", "planned", "n/a", "info", "core"]);
        assert_eq!(PARTIAL.note(), "b; not yet: c");
        assert_eq!(all.iter().filter(|s| s.applies()).count(), 3);
        assert_eq!(all.iter().filter_map(|s| s.proof()).count(), 2);
        assert_eq!(Status::Core { note: "f" }.note(), "f");
    }

    #[test]
    fn combining_keeps_the_less_done_and_skips_what_does_not_apply() {
        assert_eq!(DONE.combine(PARTIAL), PARTIAL);
        assert_eq!(PLANNED.combine(DONE), PLANNED);
        assert_eq!(NOT_A_VM.combine(DONE), DONE);
        assert_eq!(DONE.combine(NOT_A_CONTAINER), DONE);
        assert_eq!(NOT_A_VM.combine(NOT_A_CONTAINER), NOT_A_VM);
    }
}
