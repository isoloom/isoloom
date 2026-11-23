//! Folders of the computer that runs the environment, shared into a VM both ways (`host_folders`): each
//! provider's own two-way shared folder, set in that provider's block (its `override`), so only
//! the provider `vagrant up` runs shares them. A provider with none stops `vagrant up` with the
//! reason before creating the VM, rather than fall back to a one-way copy (rsync).
//!
//! - VirtualBox: `virtualbox` (vboxsf, the box's Guest Additions). Vagrant mounts it as the VM's
//!   SSH user and group with the folder's own mode on the computer (a home folder's 0755: only
//!   that user writes); a read-write one is mounted group-writable (`dmode=0775`, `fmode=0664`), so
//!   another account of the VM writes there once in that group, as on the other providers.
//! - VMware Workstation and Fusion: `vmware` (HGFS, the box's VMware Tools).
//! - Parallels: `parallels` (the box's Parallels Tools).
//! - UTM (`vagrant_utm` 0.1.3 or later): `utm` (QEMU's 9p shares, `mapped-xattr`). Vagrant mounts
//!   it as the VM's SSH user and group with the folder's own mode on the computer (0755: only that
//!   user writes), and 9p takes no mode at its mount, so a Linux VM's read-write one is made
//!   group-writable at each `vagrant up` ([`group_writes`]), as VirtualBox's is. The mode is the
//!   VM's own, kept in the folder's extended attributes: the folder's on the computer is untouched.
//! - QEMU (vagrant-qemu 0.7.0 or later): `qemu_virtiofs`, from `virtiofsd` on the host; it mounts
//!   every share read-write, so a read-only folder is refused there. virtiofsd works as the
//!   computer's user for the guest's user 1000 and, where it translates ids, for every other
//!   account of the VM too (`IsoloomQemu.virtiofs_squash`), so another account writes there as on
//!   the other providers.
//! - libvirt: `virtiofs` (libvirt 6.2 or later), the VM's memory shared with `virtiofsd`.
//! - Hyper-V: none Isoloom uses: its two-way folder is SMB, which asks for a Windows account's
//!   password at every `vagrant up`.
//! - ESXi: none: its VMs run on the ESXi host, not on the computer that runs the environment.

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::HostFolder;

use crate::ruby::{Call, Provision, Ruby, Value};
use crate::vms::provider::{self, Provider};

/// How `provider` shares a folder of the kind asked (`read_only`: one is read-only): its synced
/// folder `type:` and the keyword a read-only mount takes `ro` under, or why it can't.
fn shared_as(provider: Provider, read_only: bool) -> Result<(&'static str, &'static str), &'static str> {
    match provider.profile().share {
        provider::Share::Both { kind, ro_option } => Ok((kind, ro_option)),
        provider::Share::ReadWrite { why, .. } if read_only => Err(why),
        provider::Share::ReadWrite { kind, .. } => Ok((kind, "mount_options")),
        provider::Share::None(why) => Err(why),
    }
}

/// Why `provider` can't share a folder of the computer that runs the environment into a VM, if it can't:
/// it has no two-way shared folder Isoloom uses, or (`read_only`) it can't make one read-only.
fn why_not(provider: Provider, read_only: bool) -> Option<String> {
    shared_as(provider, read_only).err().map(str::to_owned)
}

/// The providers that share the folders (`read_only`: one is read-only), with their kind and
/// read-only keyword, in the order the Vagrantfile writes them.
fn sharing(read_only: bool) -> impl Iterator<Item = (Provider, (&'static str, &'static str))> {
    Provider::ALL.into_iter().filter_map(move |p| Some((p, shared_as(p, read_only).ok()?)))
}

/// One folder shared into a VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Share {
    /// The folder, as `from` writes it (relative to the project, `~/...` or absolute).
    pub(crate) from: String,
    /// Where the VM mounts it.
    pub(crate) guest: String,
    /// The VM only reads it.
    pub(crate) read_only: bool,
}

impl Share {
    /// A machine's own host folder, at its path.
    pub(crate) fn of(f: &HostFolder) -> Share {
        Share {
            from: f.from.clone(),
            guest: f.path.clone(),
            read_only: f.read_only,
        }
    }
}

/// libvirt's memory for a VM that shares folders: virtiofsd reads the VM's memory, so it is
/// shared, and backed by a memory file.
fn libvirt_shared_memory(r: &mut Ruby) {
    r.call(Call::new("v.memorybacking").arg(Value::sym("access")).kw("mode", Value::str("shared")));
    r.call(Call::new("v.memorybacking").arg(Value::sym("source")).kw("type", Value::str("memfd")));
}

/// The providers' blocks sharing `shares` into the VM `scope` configures (`m`, `config`), then a
/// trigger stopping `vagrant up` on the providers that can't. Nothing without shares.
pub(crate) fn render(r: &mut Ruby, scope: &str, shares: &[Share]) {
    if shares.is_empty() {
        return;
    }
    r.comment("This computer's folders (`host_folders`), both ways: each provider's own shared folder.");
    let read_only = shares.iter().any(|s| s.read_only);
    for (p, how) in sharing(read_only) {
        let head = Call::new(format!("{scope}.vm.provider")).arg(Value::str(p.id()));
        let params = if matches!(p, Provider::Libvirt | Provider::Qemu) { "v, o" } else { "_, o" };
        r.block(head, params, |r| {
            if p == Provider::Libvirt {
                libvirt_shared_memory(r);
            }
            if p == Provider::Qemu {
                virtiofs_every_account(r);
            }
            if p == Provider::VmwareDesktop {
                // VMware's folders only, picked by Vagrant itself (no `type:`, see `synced_folder`).
                r.assign("o.vm.allowed_synced_folder_types", Value::array([Value::sym("vmware")]));
            }
            for s in shares {
                r.call(synced_folder(p, how, s));
            }
        });
    }
    refuse_elsewhere(r, scope, read_only);
}

/// QEMU's `VirtioFS` share open to every account of the VM, as the other providers' are:
/// vagrant-qemu's virtiofsd maps its guest user and group (set here to the ids it defaults to)
/// to this computer's, and refuses any other id, so every other one is squashed to them
/// (`IsoloomQemu.virtiofs_squash`, nothing where virtiofsd translates no ids).
fn virtiofs_every_account(r: &mut Ruby) {
    r.assign("v.virtiofs_guest_uid", Value::raw("IsoloomQemu::GUEST_ID"));
    r.assign("v.virtiofs_guest_gid", Value::raw("IsoloomQemu::GUEST_ID"));
    r.assign("v.extra_virtiofsd_args", Value::raw("IsoloomQemu.virtiofs_squash"));
}

/// The provisioner's name: what `vagrant up --provision-with` takes.
pub(crate) const GROUP_WRITES: &str = "host_folders";

/// The script, given each read-write folder's mount point: each one that is a 9p mount (UTM's)
/// made group-writable. Any other mount (VirtualBox's, already group-writable; virtiofs, which
/// maps ids instead) is left alone.
const GROUP_WRITES_SCRIPT: &str = r#"set -eu
for folder in "$@"; do
  if awk -v f="$folder" '$2 == f && $3 == "9p" { found = 1 } END { exit !found }' /proc/mounts; then
    chmod g+w "$folder"
  fi
done
"#;

/// `m.vm.provision "host_folders"`, run at every `vagrant up`, for a Linux VM's read-write
/// `shares`: each that UTM mounts (9p) made group-writable, so another account of the VM writes
/// there once in its group, as on the other providers. Nothing without a read-write share.
pub(crate) fn group_writes(r: &mut Ruby, shares: &[Share]) {
    let folders: Vec<Value> = shares.iter().filter(|s| !s.read_only).map(|s| Value::str(&s.guest)).collect();
    if folders.is_empty() {
        return;
    }
    r.provision(
        Provision::named("m", GROUP_WRITES, "shell")
            .kw("run", Value::str("always"))
            .kw("inline", Value::str(GROUP_WRITES_SCRIPT))
            .kw("args", Value::array(folders)),
    );
}

/// VirtualBox's mount of a read-write folder: its group writes too ([`synced_folder`]).
const VIRTUALBOX_GROUP_WRITES: [&str; 2] = ["dmode=0775", "fmode=0664"];

/// VMware's mount of a read-write folder (`vmhgfs-fuse`): its group writes too. HGFS reports the
/// host folder's own modes (0755 on a Mac), so without it only the VM's owning account wrote
/// there; another one in its group (the attack box's `player`) was refused. vagrant-vmware-desktop
/// mounts every folder of one owner from one `vmhgfs-fuse` mount, so the first folder's options
/// hold for all of them.
const VMWARE_GROUP_WRITES: [&str; 1] = ["umask=0002"];

/// `o.vm.synced_folder <the folder>, <guest>, type: <kind>, create: true`, read-only as the
/// provider's mount takes it; VirtualBox's and VMware's read-write ones group-writable.
///
/// VMware Desktop's carries no `type:`: Vagrant checks a folder of an explicit type with
/// `usable?(machine, true)`, which vagrant-vmware-desktop (3.0.5) defines with one argument, so
/// `vagrant up` stopped on an `ArgumentError`. A folder without a type is given the first usable
/// one of `allowed_synced_folder_types` (VMware's alone, set in its block), asked with one argument.
fn synced_folder(p: Provider, (kind, ro_option): (&str, &str), s: &Share) -> Call {
    let call = Call::new("o.vm.synced_folder")
        .arg(Value::raw(format!("File.expand_path({}, ROOT)", Value::str(&s.from))))
        .arg(Value::str(&s.guest));
    let call = if p == Provider::VmwareDesktop {
        call
    } else {
        call.kw("type", Value::str(kind))
    };
    let call = call.kw("create", true);
    match (s.read_only, p) {
        (false, Provider::VirtualBox) => call.kw("mount_options", Value::array(VIRTUALBOX_GROUP_WRITES.map(Value::str))),
        (false, Provider::VmwareDesktop) => call.kw("mount_options", Value::array(VMWARE_GROUP_WRITES.map(Value::str))),
        (false, _) => call,
        (true, Provider::Libvirt) => call.kw(ro_option, Value::str("ro")),
        (true, _) => call.kw(ro_option, Value::array([Value::str("ro")])),
    }
}

/// A trigger stopping `vagrant up`, with the reason and before the VM is created, on every
/// provider that doesn't share the folders: the ones with no block above (Hyper-V, ESXi, QEMU for
/// a read-only one) and any provider Isoloom doesn't know.
fn refuse_elsewhere(r: &mut Ruby, scope: &str, read_only: bool) {
    let reason = |why: &str| format!("host folders (`host_folders`): {why}");
    let refused = Provider::ALL
        .into_iter()
        .filter_map(|p| Some((Value::sym(p.id()), Value::str(&reason(&why_not(p, read_only)?)))));
    let why = Value::hash(refused);
    let shared = Value::array(sharing(read_only).map(|(p, _)| Value::sym(p.id())));
    // Any other provider: its name is Vagrant's, a symbol, never the spec's text.
    let unknown = reason("Isoloom knows no two-way shared folder of the `#{machine.provider_name}` provider");
    r.block(Call::new(format!("{scope}.trigger.before")).arg(Value::sym("up")), "t", |r| {
        r.block("t.ruby", "_, machine", |r| {
            r.assign("why", Value::raw(format!("{why}[machine.provider_name]")));
            r.line(format!("why ||= \"{unknown}\" unless {shared}.include?(machine.provider_name)"));
            r.line("abort(\"#{machine.name}: \" + why) if why");
        });
    });
}

/// Why the VMs of `env` (the `vagrant` target's) can't have their host folders under `provider`,
/// if they can't: the first VM with one the provider can't share (it shares none, or none
/// read-only).
#[must_use]
pub(crate) fn refusal(env: &Environment, provider: Provider) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm && !mp.host_folders.is_empty())
        .find_map(|(name, mp)| {
            let why = why_not(provider, mp.host_folders.iter().any(|f| f.read_only))?;
            Some(refused(name, &why))
        })
}

/// Machine `name`'s host folders refused, `why`.
fn refused(name: &str, why: &str) -> String {
    format!("machine `{name}` mounts a folder of the computer that runs the environment (`host_folders`): {why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruby(shares: &[Share]) -> String {
        let mut r = Ruby::new();
        render(&mut r, "m", shares);
        r.finish()
    }

    fn share(from: &str, guest: &str, read_only: bool) -> Share {
        Share {
            from: from.into(),
            guest: guest.into(),
            read_only,
        }
    }

    /// Provider `name`'s block of the rendered Ruby (`m.vm.provider "<name>" do |…|`).
    fn provider<'r>(r: &'r isoloom_test_support::RubyBlock, name: &str) -> Option<&'r isoloom_test_support::RubyBlock> {
        r.find(&format!("m.vm.provider \"{name}\" do |")).into_iter().next()
    }

    /// The trigger's statements that stop `vagrant up` on a provider that doesn't share: the table
    /// of reasons, the unknown provider's, the abort.
    fn refusals(r: &isoloom_test_support::RubyBlock) -> Vec<&str> {
        r.block("m.trigger.before :up do |t|")
            .block("t.ruby do |_, machine|")
            .statements
            .iter()
            .map(String::as_str)
            .collect()
    }

    /// A share that is `drwxrwxr-x 1000:1000` must let another account of the VM in group 1000
    /// (uid 1001) write, but vagrant-qemu's virtiofsd maps uid and gid 1000 alone, so its writes
    /// would fail with EPERM. QEMU's block sets the ids it maps and squashes every other one to
    /// this computer's user (`IsoloomQemu.virtiofs_squash`).
    #[test]
    fn qemus_share_lets_every_account_of_the_vm_write_as_the_others_do() {
        let out = ruby(&[share("shared", "/srv/shared", false)]);
        let r = isoloom_test_support::ruby_blocks(&out);
        let qemu = provider(&r, "qemu").expect("QEMU's block");
        for line in [
            "v.virtiofs_guest_uid = IsoloomQemu::GUEST_ID",
            "v.virtiofs_guest_gid = IsoloomQemu::GUEST_ID",
            "v.extra_virtiofsd_args = IsoloomQemu.virtiofs_squash",
            "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", type: \"qemu_virtiofs\", create: true",
        ] {
            assert!(qemu.statements.iter().any(|s| s == line), "{line}: {:?}", qemu.statements);
        }
        // Only QEMU's block takes its config: VirtualBox's has the folder alone.
        let vbox = provider(&r, "virtualbox").expect("VirtualBox's block");
        assert!(vbox.statements.iter().all(|s| !s.contains("virtiofs")), "{:?}", vbox.statements);
    }

    /// UTM's 9p share is the folder's mode on the computer (0755), handed to the VM's SSH user:
    /// another account of the VM in its group couldn't write. A Linux VM's read-write folders are
    /// made group-writable where 9p mounts them, at every `vagrant up`; a read-only one isn't.
    #[test]
    fn a_read_write_folder_utm_mounts_is_group_writable_at_every_up() {
        let mut r = Ruby::new();
        group_writes(
            &mut r,
            &[
                share("shared", "/shared", false),
                share("~/notes", "/notes", true),
                share("/srv/drop", "/srv/drop", false),
            ],
        );
        let out = r.finish();
        assert!(out.contains("m.vm.provision \"host_folders\", type: \"shell\""), "{out}");
        assert!(out.contains("run: \"always\""), "{out}");
        assert!(out.contains("args: [\"/shared\", \"/srv/drop\"]"), "{out}");
        let mut none = Ruby::new();
        group_writes(&mut none, &[share("~/notes", "/notes", true)]);
        assert!(none.finish().trim().is_empty(), "no read-write folder, no provisioner");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_script_makes_a_9p_mount_group_writable_and_leaves_any_other_alone() {
        let parsed = std::process::Command::new("sh").args(["-n", "-c", GROUP_WRITES_SCRIPT]).status().unwrap();
        assert!(parsed.success(), "not valid sh");
        // A folder that isn't a mount point at all: nothing changes, and it succeeds.
        let dir = tempfile::Builder::new().prefix("isoloom-not-9p-").tempdir().unwrap();
        let folder = dir.path().to_str().unwrap();
        let mode = || std::fs::metadata(folder).unwrap().permissions();
        let before = mode();
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", GROUP_WRITES_SCRIPT, "host_folders", folder])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(mode(), before, "not a 9p mount: left alone");
    }

    #[test]
    fn each_provider_shares_both_ways_with_its_own_kind_and_the_others_refuse() {
        let out = ruby(&[share("shared", "/srv/shared", false), share("~/notes", "/notes", false)]);
        let r = isoloom_test_support::ruby_blocks(&out);
        let folders = |name: &str| provider(&r, name).map(|b| b.statements("o.vm.synced_folder ")).unwrap_or_default();
        // VirtualBox's read-write mount is group-writable.
        assert_eq!(
            folders("virtualbox").first().copied(),
            Some(
                "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", type: \"virtualbox\", create: true, mount_options: [\"dmode=0775\", \"fmode=0664\"]"
            )
        );
        // VMware's folders carry no type (its plugin's `usable?` takes one argument, which Vagrant
        // passes only to pick a type), VMware's the only one allowed; its read-write mount is
        // group-writable.
        let vmware = provider(&r, "vmware_desktop").expect("VMware's block");
        assert_eq!(
            vmware.statements,
            [
                "o.vm.allowed_synced_folder_types = [:vmware]",
                "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", create: true, mount_options: [\"umask=0002\"]",
                "o.vm.synced_folder File.expand_path(\"~/notes\", ROOT), \"/notes\", create: true, mount_options: [\"umask=0002\"]",
            ]
        );
        for (name, kind) in [("parallels", "parallels"), ("utm", "utm"), ("qemu", "qemu_virtiofs"), ("libvirt", "virtiofs")] {
            assert_eq!(
                folders(name),
                [
                    format!("o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", type: \"{kind}\", create: true"),
                    format!("o.vm.synced_folder File.expand_path(\"~/notes\", ROOT), \"/notes\", type: \"{kind}\", create: true"),
                ],
                "{name}"
            );
        }
        let libvirt = provider(&r, "libvirt").expect("libvirt's block");
        assert!(
            libvirt.statements.iter().any(|s| s == "v.memorybacking :access, mode: \"shared\""),
            "{:?}",
            libvirt.statements
        );
        assert!(provider(&r, "hyperv").is_none(), "{out}");
        let refused = refusals(&r);
        let [why, _, abort] = refused.as_slice() else { panic!("{refused:?}") };
        assert!(
            why.starts_with("why = { :hyperv => \"host folders (`host_folders`): the `hyperv` provider's only two-way shared folder is SMB")
                && why.contains(", :vmware_esxi => \"host folders (`host_folders`): the `vmware_esxi` provider's VMs run on the ESXi host")
                && !why.contains(":qemu =>"),
            "{why}"
        );
        assert_eq!(*abort, "abort(\"#{machine.name}: \" + why) if why");
    }

    #[test]
    fn a_provider_isoloom_does_not_know_stops_vagrant_up_too() {
        let unknown = |shared: &str| {
            format!(
                "why ||= \"host folders (`host_folders`): Isoloom knows no two-way shared folder of the `#{{machine.provider_name}}` provider\" unless [{shared}].include?(machine.provider_name)"
            )
        };
        let out = ruby(&[share("shared", "/srv/shared", false)]);
        let read_write = isoloom_test_support::ruby_blocks(&out);
        assert_eq!(
            refusals(&read_write)[1],
            unknown(":virtualbox, :vmware_desktop, :parallels, :libvirt, :qemu, :utm")
        );
        // A read-only folder: QEMU shares none, so it is refused like an unknown provider.
        let read_only = ruby(&[share("/srv/a", "/a", true)]);
        let read_only = isoloom_test_support::ruby_blocks(&read_only);
        assert_eq!(refusals(&read_only)[1], unknown(":virtualbox, :vmware_desktop, :parallels, :libvirt, :utm"));
    }

    #[test]
    fn a_read_only_folder_is_mounted_read_only_and_qemu_which_cannot_refuses() {
        let out = ruby(&[share("/srv/a", "/a", true)]);
        let r = isoloom_test_support::ruby_blocks(&out);
        let folders = |name: &str| provider(&r, name).map(|b| b.statements("o.vm.synced_folder ")).unwrap_or_default();
        assert_eq!(
            folders("virtualbox"),
            ["o.vm.synced_folder File.expand_path(\"/srv/a\", ROOT), \"/a\", type: \"virtualbox\", create: true, mount_options: [\"ro\"]"]
        );
        assert_eq!(
            folders("libvirt"),
            ["o.vm.synced_folder File.expand_path(\"/srv/a\", ROOT), \"/a\", type: \"virtiofs\", create: true, mount_opts: \"ro\""]
        );
        assert!(provider(&r, "qemu").is_none(), "{out}");
        let refused = refusals(&r);
        assert!(
            refused[0].contains(", :qemu => \"host folders (`host_folders`): one is read-only (`read_only`)"),
            "{refused:?}"
        );
    }

    #[test]
    fn nothing_without_a_share() {
        assert_eq!(ruby(&[]), "");
    }
}
