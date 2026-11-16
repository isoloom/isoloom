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
//! - UTM (`vagrant_utm` 0.1.3 or later): `utm` (QEMU's 9p shares).
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

use crate::ruby::{Call, Ruby, Value};
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

/// VirtualBox's mount of a read-write folder: its group writes too ([`synced_folder`]).
const VIRTUALBOX_GROUP_WRITES: [&str; 2] = ["dmode=0775", "fmode=0664"];

/// `o.vm.synced_folder <the folder>, <guest>, type: <kind>, create: true`, read-only as the
/// provider's mount takes it; VirtualBox's read-write one group-writable.
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
        // passes only to pick a type), VMware's the only one allowed.
        let vmware = provider(&r, "vmware_desktop").expect("VMware's block");
        assert_eq!(
            vmware.statements,
            [
                "o.vm.allowed_synced_folder_types = [:vmware]",
                "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", create: true",
                "o.vm.synced_folder File.expand_path(\"~/notes\", ROOT), \"/notes\", create: true",
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
