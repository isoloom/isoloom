//! The parity contract's conformance runs, the pure part: the small environment each case
//! is run as ([`crate::Entry::conformance_lab`]), and the verdict on its two runs ([`verdict()`]): every
//! check passes in both forms, every fact reads the same in both. `cargo xtask catalogue check`
//! generates the environment, runs it through a runner on a container target and a VM target,
//! and reads the runs' check results back.
//!
//! A fact is run as a check of its own (`fact: <name>`): an `exec` in the service machine whose
//! output, folded into one line, the check's failure reports. It never passes on purpose: its
//! detail is the fact's value.

mod lab;
mod verdict;

pub use lab::{ConformanceLab, LabError, MACHINE, PROBE};
pub use verdict::{CaseVerdict, CheckOutcome, CheckVerdict, FactVerdict, Form, Run, verdict};
