//! Isoloom's own folder on a Linux machine, which every target writes to and the validation keeps
//! a spec's folders out of: one folder, every path under it derived from it.

/// Isoloom's state folder on a Linux machine ([`STATE_DIR`]), or a path under it: `$rest`
/// appended at compile time (`linux_state_path!("/ready")`), so a constant names a path under the
/// folder without writing the folder again. Hidden: the crates of the workspace share it.
#[doc(hidden)]
#[macro_export]
macro_rules! linux_state_path {
    () => {
        "/var/lib/isoloom"
    };
    ($rest:literal) => {
        concat!($crate::linux_state_path!(), $rest)
    };
}

/// Isoloom's state folder on a Linux machine: its marks (the ready file, what it delivered), and
/// what it keeps between boots (the inputs, the secrets, a set-up's continuation).
pub const STATE_DIR: &str = linux_state_path!();

#[cfg(test)]
mod tests {
    #[test]
    fn a_path_under_the_state_folder_is_the_folder_and_the_rest() {
        assert_eq!(super::STATE_DIR, "/var/lib/isoloom");
        assert_eq!(linux_state_path!("/ready"), "/var/lib/isoloom/ready");
    }
}
