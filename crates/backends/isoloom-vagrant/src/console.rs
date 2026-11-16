//! Each environment VM's graphical console, under every provider: what its block in the
//! Vagrantfile sets. A console listens on the loopback of the computer that runs Vagrant only,
//! never on its network.
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
//! - **Hyper-V, Parallels, UTM, ESXi**: none (UTM shows a VM's screen over SPICE only).

use isoloom_spec::model::Arch;

use crate::ruby::{Call, Ruby, Value};
use crate::vms::providers::qemu_arch;

/// The Ruby the consoles read at run time (`vagrant_console.rb`): the `IsoloomConsole` module.
pub(crate) const CONSOLE_RB: &str = include_str!("vagrant_console.rb");

/// Where every console listens: this computer's loopback.
const LOOPBACK: &str = "127.0.0.1";

/// The ports VirtualBox's remote display takes the first free one of (by the RDP port).
const VRDE_PORTS: &str = "3390-3489";

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

    fn ruby(write: impl FnOnce(&mut Ruby)) -> String {
        let mut r = Ruby::new();
        write(&mut r);
        r.finish()
    }

    /// Each provider's console listens on this computer's loopback, and nowhere else: libvirt's
    /// VNC, VMware's VNC at the runner's port, VirtualBox's remote display, QEMU's VNC (set by
    /// `IsoloomConsole.qemu`, the VM keeping a display).
    #[test]
    fn the_consoles_listen_on_the_loopback_only() {
        assert_eq!(
            ruby(libvirt),
            "v.graphics_type = \"vnc\"\nv.graphics_ip = \"127.0.0.1\"\nv.graphics_autoport = \"yes\"\n"
        );
        assert_eq!(
            ruby(|r| vmware(r, "web")),
            "if (console = IsoloomConsole.of(\"web\"))\n  v.vmx[\"RemoteDisplay.vnc.enabled\"] = \"TRUE\"\n  v.vmx[\"RemoteDisplay.vnc.ip\"] = \"127.0.0.1\"\n  v.vmx[\"RemoteDisplay.vnc.port\"] = console.fetch(\"port\").to_s\n  v.vmx[\"RemoteDisplay.vnc.password\"] = console.fetch(\"password\")\nend\n"
        );
        assert_eq!(
            ruby(virtualbox),
            "v.customize [\"modifyvm\", :id, \"--vrde\", IsoloomConsole.virtualbox_vrde? ? \"on\" : \"off\", \"--vrdeaddress\", \"127.0.0.1\", \"--vrdeport\", \"3390-3489\"]\n"
        );
        // QEMU's VNC is an argument the Ruby gives: every `-vnc` it gives listens on the loopback.
        let vnc: Vec<&str> = CONSOLE_RB.lines().filter(|l| l.contains("\"-vnc\"")).collect();
        isoloom_test_support::assert_every!(vnc, |l: &&str| l.contains("[\"-vnc\", \"127.0.0.1:0,to=99\"]"));
        assert!(CONSOLE_RB.contains(isoloom_spec::contract::CONSOLES), "the runner's variable");
    }

    /// A QEMU VM keeps a display (not the plugin's `-vga none`), its console for its own CPU, or
    /// the host's read when Vagrant reads the file.
    #[test]
    fn a_qemu_vm_shows_its_console_for_its_own_cpu_or_the_hosts() {
        let other_default = "v.other_default = %w[-parallel none -monitor none -display none]\n";
        assert_eq!(
            ruby(|r| qemu(r, Some(Arch::Arm64))),
            format!("{other_default}v.extra_qemu_args = IsoloomConsole.qemu(\"aarch64\")\n")
        );
        assert_eq!(
            ruby(|r| qemu(r, None)),
            format!("{other_default}v.extra_qemu_args = IsoloomConsole.qemu(HOST_ARCH)\n")
        );
    }
}
