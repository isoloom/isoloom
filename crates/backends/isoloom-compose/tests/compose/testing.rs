//! What the Compose backend's integration tests share.

use std::error::Error;

/// A test's outcome: a failed step is its report.
pub(crate) type Outcome<T = ()> = Result<T, Box<dyn Error>>;
