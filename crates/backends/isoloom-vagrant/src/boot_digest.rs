//! What Vagrant applies to a VM only when it boots, as a digest per VM: the run manifest's
//! `boot_digest`. Vagrant sets a VM's synced folders, networks, forwarded ports, size and provider
//! settings when it boots it (`vagrant up` of a stopped VM, `vagrant reload`); `vagrant up
//! --provision` on a running VM only runs its provisioners. A runner that finds a running VM's
//! digest changed since it booted reloads it first.
//!
//! The digest is of the statements the [`Ruby`](crate::ruby::Ruby) builder recorded while it wrote
//! the Vagrantfile ([`BootStatements`]): those of the VM's `config.vm.define` block, after those of
//! the `Vagrant.configure` block outside every define (what all its VMs share); never a comment or
//! a provisioner. A Vagrantfile with no define has Vagrant's one implicit machine, `default`.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::ruby::BootStatements;

/// Vagrant's name for the one machine of a Vagrantfile with no `config.vm.define`.
const IMPLICIT: &str = "default";

/// Each VM's boot digest (`sha256:<hex>`), by define name, of what its Vagrantfile applies at
/// boot.
pub(crate) fn of(boot: &BootStatements) -> BTreeMap<String, String> {
    if boot.defines.is_empty() {
        return BTreeMap::from([(IMPLICIT.to_owned(), digest(&boot.shared, &[]))]);
    }
    boot.defines.iter().map(|(name, own)| (name.clone(), digest(&boot.shared, own))).collect()
}

/// `sha256:<hex>` of the shared statements, then the VM's own.
fn digest(shared: &[String], own: &[String]) -> String {
    let mut h = Sha256::new();
    for l in shared
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("--"))
        .chain(own.iter().map(String::as_str))
    {
        h.update(l.as_bytes());
        h.update(b"\n");
    }
    h.finalize().iter().fold(String::from("sha256:"), |mut out, b| {
        out.extend([b >> 4, b & 0xf].into_iter().filter_map(|n| char::from_digit(u32::from(n), 16)));
        out
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::of;
    use crate::ruby::{Call, Provision, Ruby, Value};

    /// The words a test varies in [`two`]'s Vagrantfile.
    struct Words {
        timeout: u32,
        shared_script: &'static str,
        folder: Option<&'static str>,
        step: &'static str,
        comment: &'static str,
    }

    const BASE: Words = Words {
        timeout: 900,
        shared_script: "shared",
        folder: Some("/a"),
        step: "echo one",
        comment: "a comment",
    };

    /// A Vagrantfile of two VMs, `web` and `db`, written as the generators write theirs, with a
    /// shared setting and provisioner, and in `web` a comment, a synced folder in a provider block
    /// and provisioners with a heredoc and a string; its digests.
    fn two(w: &Words) -> BTreeMap<String, String> {
        let mut r = Ruby::new();
        r.assign("ROOT", Value::raw("1"));
        r.configure(|r| {
            r.assign("config.vm.boot_timeout", Value::raw(w.timeout.to_string()));
            r.provision(Provision::new("config", "shell").kw("inline", Value::str(w.shared_script)));
            r.define("web", true, |r| {
                r.assign("m.vm.box", Value::str("debian"));
                r.comment(w.comment);
                if let Some(folder) = w.folder {
                    r.block(Call::new("m.vm.provider").arg(Value::str("utm")), "_, o", |r| {
                        r.line(format_args!("o.vm.synced_folder \"a\", \"{folder}\", type: \"utm\", create: true"));
                    });
                }
                r.provision(Provision::new("m", "shell").kw("name", Value::str("one")).heredoc("inline", "SH", w.step));
                r.provision(Provision::new("m", "shell").script("inline", w.step));
            });
            r.define("db", true, |r| {
                r.assign("m.vm.box", Value::str("debian"));
            });
        });
        of(r.boot())
    }

    #[test]
    fn each_define_has_its_own_and_its_provisioners_and_comments_count_for_nothing() {
        let d = two(&BASE);
        assert_eq!(d.keys().collect::<Vec<_>>(), ["db", "web"]);
        assert!(d["web"].starts_with("sha256:") && d["web"].len() == 7 + 64, "{}", d["web"]);
        assert_ne!(d["web"], d["db"]);
        let reworded = two(&Words {
            step: "echo uno",
            comment: "another",
            shared_script: "common",
            ..BASE
        });
        assert_eq!(reworded, d);
        // Without its synced folder (its only boot setting but the box), web is db.
        assert_eq!(two(&Words { folder: None, ..BASE })["web"], d["db"]);
    }

    #[test]
    fn a_boot_setting_changes_its_vm_and_a_shared_one_every_vm() {
        let d = two(&BASE);
        let folder = two(&Words { folder: Some("/b"), ..BASE });
        assert_ne!(folder["web"], d["web"]);
        assert_eq!(folder["db"], d["db"]);
        let shared = two(&Words { timeout: 600, ..BASE });
        assert_ne!(shared["web"], d["web"]);
        assert_ne!(shared["db"], d["db"]);
    }

    #[test]
    fn a_file_without_a_define_is_vagrants_default_machine() {
        let one = |box_name: &str, script: &str| {
            let mut r = Ruby::new();
            r.configure(|r| {
                r.assign("config.vm.box", Value::str(box_name));
                r.provision(
                    Provision::new("config", "docker")
                        .kw("type", Value::str("shell"))
                        .kw("inline", Value::str(script)),
                );
            });
            of(r.boot())
        };
        let d = one("x", "a");
        assert_eq!(d.keys().collect::<Vec<_>>(), ["default"]);
        assert_eq!(one("x", "b"), d);
        assert_ne!(one("y", "a")["default"], d["default"]);
    }

    #[test]
    fn nothing_written_outside_the_configure_block_counts() {
        let digest = |root: &str| {
            let mut r = Ruby::new();
            r.assign("ROOT", Value::raw(root));
            r.configure(|r| {
                r.assign("config.vm.box", Value::str("x"));
            });
            of(r.boot())
        };
        assert_eq!(digest("1"), digest("2"));
    }
}
