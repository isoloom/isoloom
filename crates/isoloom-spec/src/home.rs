//! Where Isoloom keeps its own state.

use std::path::PathBuf;

/// The folder Isoloom keeps its own state in: `$ISOLOOM_HOME`, else `~/.isoloom`.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("ISOLOOM_HOME") {
        return PathBuf::from(h);
    }
    let user_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    user_home.join(".isoloom")
}
