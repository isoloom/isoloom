//! What a Windows guest needs from Vagrant beyond its box: how Vagrant reaches it (WinRM, or
//! Windows' own OpenSSH through `config.winssh`), the guest OS ESXi registers it as, and a
//! virtual TPM 2.0 on the providers that give one (VirtualBox, libvirt, Hyper-V). A machine that
//! requires a TPM (`vm.tpm: true`) is refused on the others; one Windows 11 merely expects runs
//! there without (the built-in Windows 11 box installs without one).

use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::Winrm;
use isoloom_spec::windows::Tpm;

use super::provider::Provider;
use crate::ruby::{Call, Ruby, Value};

/// How Vagrant reaches a Windows box: over WinRM (`transport`, as the box answers it), or over
/// OpenSSH when the box says `winssh`. Either way the box's own `vagrant` account, and the RDP
/// port the box forwards to every interface of the host turned off (publish a service to reach
/// one, on the loopback).
pub(crate) fn communicator(r: &mut Ruby, winrm: Option<Winrm>) {
    r.call(
        Call::new("m.vm.network")
            .arg(Value::str("forwarded_port"))
            .kw("guest", 3389u16)
            .kw("host", 3389u16)
            .kw("id", Value::str("rdp"))
            .kw("disabled", true),
    );
    r.assign("m.vm.guest", Value::sym("windows"));
    match winrm {
        Some(transport) => over_winrm(r, transport),
        None => over_winssh(r),
    }
}

fn over_winrm(r: &mut Ruby, transport: Winrm) {
    r.assign("m.vm.communicator", Value::str("winrm"));
    r.assign("m.winrm.username", Value::str("vagrant"));
    r.assign("m.winrm.password", Value::str("vagrant"));
    match transport {
        Winrm::Ssl => {
            r.assign("m.winrm.transport", Value::sym("ssl"));
            r.assign("m.winrm.ssl_peer_verification", false);
        }
        Winrm::Plaintext => {
            r.assign("m.winrm.transport", Value::sym("plaintext"));
            r.assign("m.winrm.basic_auth_only", true);
        }
    }
    r.assign("m.winrm.max_tries", 30u32);
    r.assign("m.winrm.retry_delay", 10u32);
}

/// Windows' own OpenSSH server, with PowerShell as the shell the provisioners' scripts run in.
fn over_winssh(r: &mut Ruby) {
    r.assign("m.vm.communicator", Value::str("winssh"));
    r.assign("m.winssh.username", Value::str("vagrant"));
    r.assign("m.winssh.password", Value::str("vagrant"));
    r.assign("m.winssh.shell", Value::str("powershell"));
}

/// The guest OS ESXi registers a Windows VM as (its `guestOS`), so it offers the right virtual
/// hardware and tools; a Linux box keeps the one it was built with.
pub(crate) fn esxi_guestos(os: &str) -> Option<&'static str> {
    Some(match os {
        "windows-10" => "windows9-64",
        "windows-11" => "windows11-64",
        "windows-server-2008r2" => "windows7srv-64",
        "windows-server-2012r2" => "windows8srv-64",
        "windows-server-2016" => "windows9srv-64",
        "windows-server-2019" => "windows2019srv-64",
        "windows-server-2022" => "windows2019srvnext-64",
        "windows-server-2025" => "windows2022srvnext-64",
        _ => return None,
    })
}

/// libvirt's emulated TPM 2.0 (swtpm on the host), on the CRB interface Windows 11 expects.
pub(crate) fn libvirt_tpm(r: &mut Ruby) {
    r.assign("v.tpm_type", Value::str("emulator"));
    r.assign("v.tpm_model", Value::str("tpm-crb"));
    r.assign("v.tpm_version", Value::str("2.0"));
}

/// VirtualBox's TPM 2.0 (VirtualBox 7 and later), through `modifyvm`.
pub(crate) fn virtualbox_tpm(r: &mut Ruby) {
    let args = ["modifyvm", ":id", "--tpm-type", "2.0"].map(|a| if a == ":id" { Value::sym("id") } else { Value::str(a) });
    r.call(Call::new("v.customize").arg(Value::array(args)));
}

/// Why `provider` can't run this environment's VMs, if it can't: a machine that requires a TPM
/// on a provider that gives none.
pub(crate) fn tpm_refusal(env: &Environment, provider: Provider<'_>) -> Option<String> {
    if provider.has_tpm() {
        return None;
    }
    let (name, _) = env
        .machines
        .iter()
        .find(|(_, mp)| mp.shape == MachineShape::Vm && mp.tpm == Some(Tpm::Required))?;
    Some(format!(
        "machine `{name}` asks for a virtual TPM (`vm.tpm: true`): Vagrant's {} provider gives none (VirtualBox, libvirt and Hyper-V do)",
        provider.id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_windows_os_has_an_esxi_guest_os_and_linux_keeps_its_box_s() {
        for os in isoloom_spec::model::KNOWN_OS.iter().filter(|o| o.starts_with("windows")) {
            assert!(esxi_guestos(os).is_some(), "{os}");
        }
        assert_eq!(esxi_guestos("debian-12"), None);
    }

    #[test]
    fn virtualbox_s_tpm_is_a_modifyvm() {
        let mut r = Ruby::new();
        virtualbox_tpm(&mut r);
        assert_eq!(r.finish(), "v.customize [\"modifyvm\", :id, \"--tpm-type\", \"2.0\"]\n");
    }
}
