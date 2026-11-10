//! Each environment VM's graphical console, under every provider: what its block in the
//! Vagrantfile sets, and what the run manifest says of it (`machines[].consoles`). A console
//! listens on the loopback of the computer that runs Vagrant only, never on its network.
//!
//! - **libvirt**: VNC on `127.0.0.1`, its port picked by libvirt (`graphics_autoport`); the runner
//!   asks libvirt for it (`virsh domdisplay`).
//! - **QEMU** (vagrant-qemu): VNC on `127.0.0.1` at the first free display from 5900 (`-vnc
//!   127.0.0.1:0,to=99`), with a display and a USB tablet the plugin's defaults leave out (its
//!   `-vga none`); the runner asks the VM's QEMU monitor for the port (`info vnc`).
//! - **VMware** Workstation and Fusion: their VNC server (`RemoteDisplay.vnc.*` in the `.vmx`) on
//!   `127.0.0.1`, at the port and with the password the runner chose for the run and hands the
//!   Vagrantfile in `ISOLOOM_CONSOLES`; off when it gave none (Vagrant run by hand).
//! - **VirtualBox**: its remote display (VRDE, RDP) on `127.0.0.1`, only when the Oracle VM
//!   VirtualBox Extension Pack is installed (VirtualBox alone has no VRDE server); off otherwise.
//! - **Hyper-V, Parallels, ESXi**: none, each saying why ([`none_note`]). UTM runs no
//!   environment's VMs (the Vagrantfile has no block for it).

use std::collections::BTreeSet;

use isoloom_manifest::{Console, ConsoleProtocol, ConsoleSource, Provider as ManifestProvider};
use isoloom_spec::model::Arch;

use crate::ruby::{Call, Ruby, Value};
use crate::vms::provider::Provider;
use crate::vms::providers::qemu_arch;

/// The Ruby the consoles read at run time (`vagrant_console.rb`): the `IsoloomConsole` module.
pub(crate) const CONSOLE_RB: &str = include_str!("vagrant_console.rb");

/// Where every console listens: this computer's loopback.
const LOOPBACK: &str = "127.0.0.1";

/// The ports VirtualBox's remote display takes the first free one of (by the RDP port).
const VRDE_PORTS: &str = "3390-3489";

/// Why `provider` gives a VM no graphical console Isoloom offers, when it gives none.
pub(crate) fn none_note(provider: Provider<'_>) -> Option<&'static str> {
    match provider {
        Provider::HyperV => {
            Some("Hyper-V has no VNC or remote display server of its own; a Windows machine's own Remote Desktop is the way, when the spec opens its port")
        }
        Provider::Parallels => Some("Isoloom sets up no graphical console on Parallels Desktop (its VNC server stays off)"),
        Provider::Utm => Some("UTM shows a VM's screen over SPICE only, which Isoloom doesn't relay"),
        Provider::VmwareEsxi => Some(isoloom_render::esxi::NO_CONSOLE),
        Provider::VirtualBox | Provider::VmwareDesktop | Provider::Libvirt | Provider::Qemu | Provider::Other(_) => None,
    }
}

/// An environment VM's consoles, one per provider of the Vagrantfile (every one that runs an
/// environment's VMs: not UTM), in its order.
pub(crate) fn consoles() -> Vec<Console> {
    Provider::ALL
        .iter()
        .filter(|k| k.provider.runs_vms())
        .map(|k| {
            let only = BTreeSet::from([k.manifest]);
            if let Some(why) = none_note(k.provider) {
                return Console::none(only, why);
            }
            let (protocol, source) = match k.manifest {
                ManifestProvider::Libvirt => (ConsoleProtocol::Vnc, ConsoleSource::Libvirt {}),
                ManifestProvider::Qemu => (ConsoleProtocol::Vnc, ConsoleSource::QemuMonitor {}),
                ManifestProvider::VmwareDesktop => (ConsoleProtocol::Vnc, ConsoleSource::Runner {}),
                _ => (ConsoleProtocol::Rdp, ConsoleSource::Vrde {}),
            };
            let mut console = Console::at(only, protocol, source);
            if k.manifest == ManifestProvider::Virtualbox {
                console.note = Some(
                    "on only when the Oracle VM VirtualBox Extension Pack is installed as the VM boots (VirtualBox has no remote display server without it)"
                        .into(),
                );
            }
            console
        })
        .collect()
}

/// libvirt's block: VNC on the loopback, its port libvirt's pick.
pub(crate) fn libvirt(r: &mut Ruby) {
    r.assign("v.graphics_type", Value::str("vnc"));
    r.assign("v.graphics_ip", Value::str(LOOPBACK));
    r.assign("v.graphics_autoport", Value::str("yes"));
}

/// QEMU's block: the plugin's defaults but its `-vga none` (so the VM has a display), and the
/// VNC display with what the VM's CPU needs to show and point on it (`arch`: the machine's own,
/// else the host's).
pub(crate) fn qemu(r: &mut Ruby, arch: Option<Arch>) {
    r.assign("v.other_default", Value::raw("%w[-parallel none -monitor none -display none]"));
    let arch = arch.map_or_else(|| Value::raw("HOST_ARCH"), |a| Value::str(qemu_arch(a)));
    r.assign("v.extra_qemu_args", Value::raw(format!("IsoloomConsole.qemu({arch})")));
}

/// VMware's block: its VNC server on the loopback at the port and with the password the runner
/// chose for `machine`, when it chose one.
pub(crate) fn vmware(r: &mut Ruby, machine: &str) {
    r.if_block(format_args!("(console = IsoloomConsole.of({}))", Value::str(machine)), |r| {
        r.assign("v.vmx[\"RemoteDisplay.vnc.enabled\"]", Value::str("TRUE"));
        r.assign("v.vmx[\"RemoteDisplay.vnc.ip\"]", Value::str(LOOPBACK));
        r.assign("v.vmx[\"RemoteDisplay.vnc.port\"]", Value::raw("console.fetch(\"port\").to_s"));
        r.assign("v.vmx[\"RemoteDisplay.vnc.password\"]", Value::raw("console.fetch(\"password\")"));
    });
}

/// VirtualBox's block: its remote display on the loopback when the extension pack gives one, off
/// otherwise (a box's own setting never left on).
pub(crate) fn virtualbox(r: &mut Ruby) {
    let words = [
        Value::str("modifyvm"),
        Value::raw(":id"),
        Value::str("--vrde"),
        Value::raw(format!("IsoloomConsole.virtualbox_vrde? ? {} : {}", Value::str("on"), Value::str("off"))),
        Value::str("--vrdeaddress"),
        Value::str(LOOPBACK),
        Value::str("--vrdeport"),
        Value::str(VRDE_PORTS),
    ];
    r.call(Call::new("v.customize").arg(Value::array(words)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_says_whether_a_vm_has_a_console_and_how() {
        let cs = consoles();
        assert_eq!(cs.len(), Provider::ALL.iter().filter(|k| k.provider.runs_vms()).count());
        assert!(!cs.iter().any(|c| c.providers.contains(&ManifestProvider::Utm)), "the Vagrantfile has no UTM block");
        let of = |p: ManifestProvider| cs.iter().find(|c| c.providers.contains(&p)).unwrap();
        assert_eq!(of(ManifestProvider::Libvirt).source, Some(ConsoleSource::Libvirt {}));
        assert_eq!(of(ManifestProvider::Qemu).source, Some(ConsoleSource::QemuMonitor {}));
        assert_eq!(of(ManifestProvider::VmwareDesktop).source, Some(ConsoleSource::Runner {}));
        let vbox = of(ManifestProvider::Virtualbox);
        assert_eq!((vbox.protocol, &vbox.source), (ConsoleProtocol::Rdp, &Some(ConsoleSource::Vrde {})));
        assert!(vbox.note.as_deref().is_some_and(|n| n.contains("Extension Pack")));
        for p in [
            ManifestProvider::Hyperv,
            ManifestProvider::Parallels,
            ManifestProvider::VmwareEsxi,
        ] {
            let c = of(p);
            assert_eq!(c.protocol, ConsoleProtocol::None, "{p:?}");
            assert!(c.note.is_some() && c.source.is_none(), "{p:?}");
        }
        assert!(
            cs.iter()
                .all(|c| c.providers.len() == 1 && (c.protocol == ConsoleProtocol::None) == c.source.is_none())
        );
    }

    #[test]
    fn the_consoles_listen_on_the_loopback_only() {
        let mut r = Ruby::new();
        libvirt(&mut r);
        qemu(&mut r, Some(Arch::Arm64));
        vmware(&mut r, "web");
        virtualbox(&mut r);
        let text = r.finish();
        assert!(text.contains("v.graphics_type = \"vnc\"\nv.graphics_ip = \"127.0.0.1\"\n"), "{text}");
        assert!(text.contains("v.extra_qemu_args = IsoloomConsole.qemu(\"aarch64\")"), "{text}");
        assert!(!text.contains("-vga none"), "the VM keeps a display: {text}");
        assert!(text.contains("if (console = IsoloomConsole.of(\"web\"))\n  v.vmx[\"RemoteDisplay.vnc.enabled\"] = \"TRUE\"\n  v.vmx[\"RemoteDisplay.vnc.ip\"] = \"127.0.0.1\"\n"), "{text}");
        assert!(
            text.contains("v.customize [\"modifyvm\", :id, \"--vrde\", IsoloomConsole.virtualbox_vrde? ? \"on\" : \"off\", \"--vrdeaddress\", \"127.0.0.1\", \"--vrdeport\", \"3390-3489\"]"),
            "{text}"
        );
        let ruby = CONSOLE_RB;
        assert!(ruby.contains("\"127.0.0.1:0,to=99\""), "QEMU's VNC on the loopback");
        assert!(ruby.contains(isoloom_manifest::contract::CONSOLES), "the runner's variable");
    }

    #[test]
    fn a_qemu_vm_of_the_hosts_cpu_reads_it_at_run_time() {
        let mut r = Ruby::new();
        qemu(&mut r, None);
        assert!(r.finish().contains("v.extra_qemu_args = IsoloomConsole.qemu(HOST_ARCH)"));
    }
}
