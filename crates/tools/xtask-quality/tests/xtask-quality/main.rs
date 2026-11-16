//! The ratchet's public helpers, as the repositories' `xtask`s call them.

#![cfg(unix)]
#![expect(clippy::disallowed_methods, reason = "the tests start the commands the helpers run")]

mod process;
