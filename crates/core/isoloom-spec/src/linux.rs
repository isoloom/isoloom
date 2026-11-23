//! Isoloom's own folders on a Linux machine, which every target writes to and the validation keeps
//! a spec's folders out of: one constant each, every path under one derived from it.

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

/// Isoloom's configuration folder on a Linux machine ([`CONFIG_DIR`]), or a path under it, as
/// [`linux_state_path!`] is for the state folder. Hidden: the crates of the workspace share it.
#[doc(hidden)]
#[macro_export]
macro_rules! linux_config_path {
    () => {
        "/etc/isoloom"
    };
    ($rest:literal) => {
        concat!($crate::linux_config_path!(), $rest)
    };
}

/// Isoloom's configuration folder on a Linux machine: the scripts its boot services run, the
/// controller's key and inventory, the router's hosts file.
pub const CONFIG_DIR: &str = linux_config_path!();

/// Where the project is copied in a Linux machine, and where its steps run from.
pub const PROJECT_DIR: &str = "/opt/isoloom";

#[cfg(test)]
mod tests {
    #[test]
    fn a_path_under_the_state_folder_is_the_folder_and_the_rest() {
        assert_eq!(super::STATE_DIR, "/var/lib/isoloom");
        assert_eq!(linux_state_path!("/ready"), "/var/lib/isoloom/ready");
        assert_eq!(linux_config_path!("/hosts"), "/etc/isoloom/hosts");
    }
}
