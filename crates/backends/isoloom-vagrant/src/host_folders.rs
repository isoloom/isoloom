//! Folders of the computer that runs the environment, shared into a VM both ways (`host_folders`): each
//! provider's own two-way shared folder, set in that provider's block (its `override`), so only
//! the provider `vagrant up` runs shares them. A provider with none stops `vagrant up` with the
//! reason before creating the VM (and the run manifest's provider says it can't run the environment),
//! rather than fall back to a one-way copy (rsync).
//!
//! - VirtualBox: `virtualbox` (vboxsf, the box's Guest Additions). Vagrant mounts it as the VM's
//!   SSH user and group with the folder's own mode on the computer (a home folder's 0755: only
//!   that user writes); a read-write one is mounted group-writable (`dmode=0775`, `fmode=0664`), so
//!   another account of the VM writes there once in that group, as on the other providers.
//! - VMware Workstation and Fusion: `vmware` (HGFS, the box's VMware Tools).
//! - Parallels: `parallels` (the box's Parallels Tools).
//! - UTM (`vagrant_utm` 0.1.3 or later): `utm` (QEMU's 9p shares).
//! - QEMU (vagrant-qemu 0.7.0 or later): `qemu_virtiofs`, from `virtiofsd` on the host; it mounts
//!   every share read-write, so a read-only folder is refused there.
//! - libvirt: `virtiofs` (libvirt 6.2 or later), the VM's memory shared with `virtiofsd`.
//! - Hyper-V: none Isoloom uses: its two-way folder is SMB, which asks for a Windows account's
//!   password at every `vagrant up`.
//! - ESXi: none: its VMs run on the ESXi host, not on the computer that runs the environment.

use isoloom_plan::{Environment, MachineShape};
use isoloom_render::container::host_folders;
use isoloom_spec::model::HostFolder;

use crate::ruby::{Call, Ruby, Value};
use crate::vms::provider::Provider;

/// Why `provider` can't share a folder of the computer that runs the environment into a VM, if it can't:
/// it has no two-way shared folder Isoloom uses, or (`read_only`) it can't make one read-only.
fn why_not(provider: Provider<'_>, read_only: bool) -> Option<String> {
    let id = provider.id();
    match provider {
        Provider::HyperV => Some(format!(
            "the `{id}` provider's only two-way shared folder is SMB, which asks for a Windows account's password at every `vagrant up`"
        )),
        Provider::VmwareEsxi => Some(format!("the `{id}` provider's VMs run on the ESXi host, not on this computer")),
        Provider::Qemu if read_only => Some(format!(
            "one is read-only (`read_only`), and the `{id}` provider (vagrant-qemu) mounts its VirtioFS shares read-write"
        )),
        _ if provider.known().is_some_and(|k| k.share.is_some()) => None,
        _ => Some(format!("Isoloom knows no two-way shared folder of the `{id}` provider")),
    }
}

/// The providers that share the folders (`read_only`: one is read-only), with their kind, in the
/// order the Vagrantfile writes them.
fn sharing(read_only: bool) -> impl Iterator<Item = (Provider<'static>, &'static str)> {
    Provider::ALL
        .into_iter()
        .filter_map(|k| Some((k.provider, k.share?)))
        .filter(move |(p, _)| why_not(*p, read_only).is_none())
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
/// shared, and backed by a memory file. libvirt can't add it to a VM that runs, so a VM that may
/// get folders later (`docker_vm`'s host, when a machine with host folders is added to the
/// environment) has it from the start.
pub(crate) fn libvirt_shared_memory(r: &mut Ruby) {
    r.call(Call::new("v.memorybacking").arg(Value::sym("access")).kw("mode", Value::str("shared")));
    r.call(Call::new("v.memorybacking").arg(Value::sym("source")).kw("type", Value::str("memfd")));
}

/// Whether [`render`] sets libvirt's shared memory itself, or the VM's own libvirt block does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibvirtMemory {
    /// [`render`] sets it, with the shares.
    WithShares,
    /// The VM's block sets it always ([`libvirt_shared_memory`]).
    Always,
}

/// The providers' blocks sharing `shares` into the VM `scope` configures (`m`, `config`), then a
/// trigger stopping `vagrant up` on the providers that can't. Nothing without shares.
pub(crate) fn render(r: &mut Ruby, scope: &str, shares: &[Share], memory: LibvirtMemory) {
    if shares.is_empty() {
        return;
    }
    r.comment("This computer's folders (`host_folders`), both ways: each provider's own shared folder.");
    let read_only = shares.iter().any(|s| s.read_only);
    for (p, kind) in sharing(read_only) {
        let head = Call::new(format!("{scope}.vm.provider")).arg(Value::str(p.id()));
        let params = if p == Provider::Libvirt { "v, o" } else { "_, o" };
        r.block(head, params, |r| {
            if p == Provider::Libvirt && memory == LibvirtMemory::WithShares {
                libvirt_shared_memory(r);
            }
            for s in shares {
                r.call(synced_folder(p, kind, s));
            }
        });
    }
    refuse_elsewhere(r, scope, read_only);
}

/// VirtualBox's mount of a read-write folder: its group writes too ([`synced_folder`]).
const VIRTUALBOX_GROUP_WRITES: [&str; 2] = ["dmode=0775", "fmode=0664"];

/// `o.vm.synced_folder <the folder>, <guest>, type: <kind>, create: true`, read-only as the
/// provider's mount takes it; VirtualBox's read-write one group-writable.
fn synced_folder(p: Provider<'_>, kind: &str, s: &Share) -> Call {
    let call = Call::new("o.vm.synced_folder")
        .arg(Value::raw(format!("File.expand_path({}, ROOT)", Value::str(&s.from))))
        .arg(Value::str(&s.guest))
        .kw("type", Value::str(kind))
        .kw("create", true);
    match (s.read_only, p) {
        (false, Provider::VirtualBox) => call.kw("mount_options", Value::array(VIRTUALBOX_GROUP_WRITES.map(Value::str))),
        (false, _) => call,
        (true, Provider::Libvirt) => call.kw("mount_opts", Value::str("ro")),
        (true, _) => call.kw("mount_options", Value::array([Value::str("ro")])),
    }
}

/// A trigger stopping `vagrant up`, with the reason and before the VM is created, on every
/// provider that doesn't share the folders: the ones with no block above (Hyper-V, ESXi, QEMU for
/// a read-only one) and any provider Isoloom doesn't know.
fn refuse_elsewhere(r: &mut Ruby, scope: &str, read_only: bool) {
    let reason = |why: &str| format!("host folders (`host_folders`): {why}");
    let refused = Provider::ALL
        .into_iter()
        .filter_map(|k| Some((Value::sym(k.provider.id()), Value::str(&reason(&why_not(k.provider, read_only)?)))));
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
pub(crate) fn refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Vm && !mp.host_folders.is_empty())
        .find_map(|(name, mp)| {
            let why = why_not(provider, mp.host_folders.iter().any(|f| f.read_only))?;
            Some(refused(name, &why))
        })
}

/// Why `docker-vm`'s VM, under Vagrant's `provider`, can't share its containers' host folders, if
/// it can't. The VM shares them read-write (a read-only one is bound read-only into its
/// container), so only the providers with no shared folder refuse.
#[must_use]
pub fn docker_vm_refusal(env: &Environment, provider: &str) -> Option<String> {
    let name = host_folders::needing_a_local_vm(env)?;
    Some(refused(name, &why_not(Provider::from(provider), false)?))
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
        render(&mut r, "m", shares, LibvirtMemory::WithShares);
        r.finish()
    }

    fn share(from: &str, guest: &str, read_only: bool) -> Share {
        Share {
            from: from.into(),
            guest: guest.into(),
            read_only,
        }
    }

    #[test]
    fn each_provider_shares_both_ways_with_its_own_kind_and_the_others_refuse() {
        let out = ruby(&[share("shared", "/srv/shared", false), share("~/notes", "/notes", false)]);
        assert!(
            out.contains(
                "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", type: \"virtualbox\", create: true, mount_options: [\"dmode=0775\", \"fmode=0664\"]\n"
            ),
            "VirtualBox's read-write mount is group-writable: {out}"
        );
        for (provider, kind) in [
            ("vmware_desktop", "vmware"),
            ("parallels", "parallels"),
            ("utm", "utm"),
            ("qemu", "qemu_virtiofs"),
            ("libvirt", "virtiofs"),
        ] {
            assert!(out.contains(&format!("m.vm.provider \"{provider}\" do |")), "{provider}: {out}");
            assert!(
                out.contains(&format!(
                    "o.vm.synced_folder File.expand_path(\"shared\", ROOT), \"/srv/shared\", type: \"{kind}\", create: true\n"
                )),
                "{provider}: {out}"
            );
        }
        assert!(out.contains("v.memorybacking :access, mode: \"shared\"\n"), "{out}");
        assert!(out.contains("File.expand_path(\"~/notes\", ROOT)"), "{out}");
        assert!(!out.contains("m.vm.provider \"hyperv\""), "{out}");
        assert!(
            out.contains(":hyperv => \"host folders (`host_folders`): the `hyperv` provider's only two-way shared folder is SMB"),
            "{out}"
        );
        assert!(
            out.contains(":vmware_esxi => \"host folders (`host_folders`): the `vmware_esxi` provider's VMs run on the ESXi host"),
            "{out}"
        );
        assert!(!out.contains(":qemu =>"), "{out}");
        assert!(out.contains("abort(\"#{machine.name}: \" + why) if why\n"), "{out}");
    }

    #[test]
    fn a_provider_isoloom_does_not_know_stops_vagrant_up_too() {
        let out = ruby(&[share("shared", "/srv/shared", false)]);
        assert!(
            out.contains(
                "why ||= \"host folders (`host_folders`): Isoloom knows no two-way shared folder of the `#{machine.provider_name}` provider\" unless [:virtualbox, :vmware_desktop, :parallels, :libvirt, :qemu, :utm].include?(machine.provider_name)\n"
            ),
            "{out}"
        );
        // A read-only folder: QEMU shares none, so it is refused like an unknown provider.
        let read_only = ruby(&[share("/srv/a", "/a", true)]);
        assert!(
            read_only.contains("unless [:virtualbox, :vmware_desktop, :parallels, :libvirt, :utm].include?"),
            "{read_only}"
        );
    }

    #[test]
    fn a_read_only_folder_is_mounted_read_only_and_qemu_which_cannot_refuses() {
        let out = ruby(&[share("/srv/a", "/a", true)]);
        assert!(out.contains("type: \"virtualbox\", create: true, mount_options: [\"ro\"]\n"), "{out}");
        assert!(out.contains("type: \"virtiofs\", create: true, mount_opts: \"ro\"\n"), "{out}");
        assert!(!out.contains("m.vm.provider \"qemu\""), "{out}");
        assert!(out.contains(":qemu => \"host folders (`host_folders`): one is read-only"), "{out}");
    }

    #[test]
    fn nothing_without_a_share() {
        assert_eq!(ruby(&[]), "");
    }
}
