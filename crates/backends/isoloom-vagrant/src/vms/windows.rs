//! What a Windows guest needs from Vagrant beyond its box: how Vagrant reaches it (WinRM, or
//! Windows' own OpenSSH through `config.winssh`), the guest OS ESXi registers it as, and a
//! virtual TPM 2.0 on the providers that give one (VirtualBox, libvirt, Hyper-V). A machine that
//! requires a TPM (`hardware.tpm: required`) is refused on the others; one Windows 11 merely expects runs
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

/// The providers that give a TPM, as a refusal names them (`VirtualBox, Hyper-V and libvirt`).
fn with_tpm() -> String {
    let names: Vec<&str> = Provider::profiles().filter(|p| p.tpm).map(|p| p.label).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.concat(),
    }
}

/// Why `provider` can't run this environment's VMs, if it can't: a machine that requires a TPM
/// on a provider that gives none.
pub(crate) fn tpm_refusal(env: &Environment, provider: Provider) -> Option<String> {
    if provider.has_tpm() {
        return None;
    }
    let (name, _) = env
        .machines
        .iter()
        .find(|(_, mp)| mp.shape == MachineShape::Vm && mp.tpm == Some(Tpm::Required))?;
    Some(format!(
        "machine `{name}` asks for a virtual TPM (`hardware.tpm: required`): Vagrant's {} provider gives none ({} do)",
        provider.id(),
        with_tpm()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtualbox_s_tpm_is_a_modifyvm() {
        let mut r = Ruby::new();
        virtualbox_tpm(&mut r);
        assert_eq!(r.finish(), "v.customize [\"modifyvm\", :id, \"--tpm-type\", \"2.0\"]\n");
    }

    /// Vagrant reaches a Windows box over WinRM as the box answers it, or over its own OpenSSH
    /// with PowerShell as the shell; always as the box's `vagrant` account, RDP's forward off.
    #[test]
    fn a_windows_box_is_reached_as_it_answers() {
        let reached = |winrm: Option<Winrm>| {
            let mut r = Ruby::new();
            communicator(&mut r, winrm);
            r.finish()
        };
        let head = "m.vm.network \"forwarded_port\", guest: 3389, host: 3389, id: \"rdp\", disabled: true\nm.vm.guest = :windows\n";
        let winrm = |transport: &str| {
            format!(
                "{head}m.vm.communicator = \"winrm\"\nm.winrm.username = \"vagrant\"\nm.winrm.password = \"vagrant\"\n{transport}m.winrm.max_tries = 30\nm.winrm.retry_delay = 10\n"
            )
        };
        assert_eq!(
            reached(Some(Winrm::Ssl)),
            winrm("m.winrm.transport = :ssl\nm.winrm.ssl_peer_verification = false\n")
        );
        assert_eq!(
            reached(Some(Winrm::Plaintext)),
            winrm("m.winrm.transport = :plaintext\nm.winrm.basic_auth_only = true\n")
        );
        assert_eq!(
            reached(None),
            format!(
                "{head}m.vm.communicator = \"winssh\"\nm.winssh.username = \"vagrant\"\nm.winssh.password = \"vagrant\"\nm.winssh.shell = \"powershell\"\n"
            )
        );
    }

    /// A machine that requires a TPM is refused, by name, on a provider that gives none, which
    /// says the ones that do; the providers with one take it.
    #[test]
    fn a_machine_requiring_a_tpm_is_refused_where_the_provider_gives_none() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  dc: { os: windows-server@2019, networks: { lan: 10 }, hardware: { tpm: required } }\n",
        );
        assert_eq!(
            tpm_refusal(&env, Provider::Qemu).as_deref(),
            Some("machine `dc` asks for a virtual TPM (`hardware.tpm: required`): Vagrant's qemu provider gives none (VirtualBox, Hyper-V and libvirt do)")
        );
        for p in [Provider::VirtualBox, Provider::HyperV, Provider::Libvirt] {
            assert_eq!(tpm_refusal(&env, p), None, "{}", p.id());
        }
    }
}

/// Which adapter of a Windows VM is on a network, for [`static_addresses`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Adapter {
    /// The one with this MAC address, as WMI writes it (`02:15:AB:00:01:FF`): Hyper-V, where
    /// Isoloom sets each adapter's MAC.
    Mac(String),
    /// The first one with IP on but no default gateway (QEMU, whose first adapter is its
    /// user-mode NAT, the way Vagrant comes in).
    NoGateway,
}

/// PowerShell giving a Windows VM its static addresses on its networks, each `(adapter, address,
/// mask)`: the adapter found through WMI, its address set unless it has it (kept across
/// restarts), as on every Windows since 2008 R2, its network then Private where the cmdlet
/// exists. Each says what it set; a missing adapter or a refused address fails it.
pub(super) fn static_addresses(nics: &[(Adapter, std::net::Ipv4Addr, std::net::Ipv4Addr)]) -> String {
    let items = nics
        .iter()
        .map(|(adapter, ip, mask)| {
            let mac = match adapter {
                Adapter::Mac(m) => m.as_str(),
                Adapter::NoGateway => "",
            };
            format!("@{{ mac = '{mac}'; ip = '{ip}'; mask = '{mask}' }}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "$ErrorActionPreference = 'Stop'\nforeach ($n in @({items})) {{\n  $all = Get-WmiObject Win32_NetworkAdapterConfiguration\n  $c = if ($n.mac) {{ $all | Where-Object {{ $_.MACAddress -eq $n.mac }} | Select-Object -First 1 }} else {{ $all | Where-Object {{ $_.IPEnabled -and -not $_.DefaultIPGateway }} | Select-Object -First 1 }}\n  if (-not $c) {{ throw ('no network adapter for ' + $n.ip + $(if ($n.mac) {{ ' with MAC ' + $n.mac }} else {{ ' without a default gateway' }})) }}\n  if (@($c.IPAddress) -notcontains $n.ip) {{\n    $r = $c.EnableStatic(@($n.ip), @($n.mask))\n    if ($r.ReturnValue -gt 1) {{ throw ('setting ' + $n.ip + ' failed: ' + $r.ReturnValue) }}\n  }}\n  if (Get-Command Set-NetConnectionProfile -ErrorAction SilentlyContinue) {{ Set-NetConnectionProfile -InterfaceIndex $c.InterfaceIndex -NetworkCategory Private -ErrorAction SilentlyContinue }}\n  'environment network: ' + $n.ip + ' on ' + $c.Description\n}}\n"
    )
}
