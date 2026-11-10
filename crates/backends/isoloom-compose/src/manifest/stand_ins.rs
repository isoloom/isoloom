//! What is done to the access machine's stand-ins around the checks: started before the runners,
//! taken down after them, the same on this computer and in a VM.

use isoloom_manifest::Failure;

/// What is done to the access machine's stand-ins around the checks, the same on this computer and
/// in a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StandInStep {
    /// The step's id.
    pub(super) id: &'static str,
    /// What `docker compose --profile check` does to them: the subcommand and its flags.
    pub(super) verb: &'static [&'static str],
    /// What the step says it is doing.
    pub(super) note: Option<&'static str>,
    /// How long the step may run, in seconds.
    pub(super) timeout_s: u32,
    /// What its failure does: a stand-in that failed to start stops the checks; one left
    /// behind is only logged.
    pub(super) failure: Failure,
}

impl StandInStep {
    /// Compose's arguments for `stand_ins`: `--profile check <verb> <stand-ins>`, the same on this
    /// computer and in a VM.
    #[must_use]
    pub(super) fn args<'a>(&self, stand_ins: &'a [String]) -> Vec<&'a str> {
        let mut args = vec!["--profile", "check"];
        args.extend(self.verb);
        args.extend(stand_ins.iter().map(String::as_str));
        args
    }
}

/// Before the runners: the stand-ins started.
pub(super) const STAND_INS_UP: StandInStep = StandInStep {
    id: "stand-ins",
    verb: &["up", "--detach", "--wait", "--no-deps"],
    note: Some("Starting the access machine's stand-in…"),
    timeout_s: 600,
    failure: Failure::Fail,
};

/// After the runners, whatever happened: the stand-ins taken down, so the access machine they
/// stand in for (brought up on demand) can take its addresses. A failure there doesn't fail the
/// checks, which already ran.
pub(super) const STAND_INS_DOWN: StandInStep = StandInStep {
    id: "stand-ins-down",
    verb: &["rm", "--stop", "--force"],
    note: None,
    timeout_s: 120,
    failure: Failure::Tolerate,
};
