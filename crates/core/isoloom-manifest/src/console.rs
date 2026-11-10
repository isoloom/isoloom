//! A machine's graphical console: its screen, keyboard and mouse as its hypervisor, its server or
//! its own OS serves them, and how a runner finds it. A console listens only on the loopback of
//! the computer that runs the environment, or is reached through an SSH tunnel the runner opens
//! there: never on the local network.
//!
//! A machine lists its consoles (`machines[].consoles`), at most one per provider (Vagrant: one
//! Vagrantfile serves every provider, each with its own), and says so when it has none and why
//! (`protocol: none`, with a `note`), so a runner can tell a user why there is no screen to show.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::Provider;
use crate::names::Identifier;
use crate::template::Template;

/// The protocol a graphical console speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleProtocol {
    /// VNC (RFB).
    Vnc,
    /// Microsoft's Remote Desktop Protocol.
    Rdp,
    /// No graphical console: the console's `note` says why.
    None,
}

/// Where a runner finds a machine's graphical console. The sources a runner reads with no
/// argument are written with none (`{"from": "libvirt"}`), and refuse one as every field does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConsoleSource {
    /// libvirt's VNC display of the VM (its domain: the machine's `native.vm_names.libvirt`), on
    /// this computer's loopback at a port libvirt picks when the VM starts: `virsh domdisplay`
    /// says which, on the connection the run's Vagrant uses (`qemu:///system` unless
    /// `LIBVIRT_DEFAULT_URI` says otherwise).
    Libvirt {},
    /// The VNC display of a VM vagrant-qemu runs, on this computer's loopback at the first free
    /// port from 5900: the VM's QEMU monitor says which (`info vnc`), on the socket vagrant-qemu
    /// keeps under its machine folder of the run's Vagrant state.
    QemuMonitor {},
    /// VirtualBox's remote display (VRDE), which speaks RDP, on this computer's loopback: on only
    /// when the Oracle VM VirtualBox Extension Pack was installed when the VM booted.
    /// `VBoxManage showvminfo` says whether it is on, and its port.
    Vrde {},
    /// A VNC server the hypervisor runs on this computer's loopback at a port and with a password
    /// the runner chooses for the run (8 characters, the most VNC's authentication reads), kept
    /// until `down`. The runner hands both to every Vagrant process of the run's operations in
    /// [`crate::contract::CONSOLES`], never to a command it gives an embedder to run.
    Runner {},
    /// Proxmox VE's own console, through its API: `vncproxy`, then `vncwebsocket`, of the VM
    /// `vm_id` on `node`. The ticket `vncproxy` gives is the VNC password too.
    ProxmoxApi {
        /// The API's address (`https://<server>:8006/`).
        endpoint: Template,
        /// The node the VM is on.
        node: Template,
        /// The VM's id.
        vm_id: Template,
        /// `true`: the server's certificate isn't verified (a self-signed one).
        insecure: Template,
        /// The user a password signs in as (`root@pam`).
        username: Template,
        /// The slot holding an API token (`user@realm!id=secret`); used when it has a value.
        token: Identifier,
        /// The slot holding the user's password, used when the token has none.
        password: Identifier,
    },
    /// A port of the machine reached through an SSH tunnel the runner opens from this computer's
    /// loopback to `host:port`, signed in to `via` as `user` with the runner's key.
    SshTunnel {
        /// The SSH host the tunnel goes through (a machine of the run, by the address the run
        /// gives it).
        via: Template,
        /// The user the runner's key lets in on `via`.
        user: Template,
        /// The machine's address, as `via` reaches it.
        host: Template,
        /// The machine's port.
        port: u16,
    },
}

impl ConsoleSource {
    /// The protocol the source speaks, when it speaks only one: VNC for libvirt, QEMU, the
    /// runner's and Proxmox's; RDP for VirtualBox's remote display; either through a tunnel.
    #[must_use]
    pub fn speaks(&self) -> Option<ConsoleProtocol> {
        match self {
            ConsoleSource::Libvirt {} | ConsoleSource::QemuMonitor {} | ConsoleSource::Runner {} | ConsoleSource::ProxmoxApi { .. } => {
                Some(ConsoleProtocol::Vnc)
            }
            ConsoleSource::Vrde {} => Some(ConsoleProtocol::Rdp),
            ConsoleSource::SshTunnel { .. } => None,
        }
    }
}

/// The account a console signs in as, when its protocol signs in to the machine's own OS (RDP
/// to a Windows machine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleAccount {
    /// The account's user name.
    pub user: Template,
    /// Its password, from an output of the run (a sensitive one: `${outputs.<name>}`).
    pub password: Template,
}

/// A machine's graphical console under some providers (Vagrant) or wherever it runs, or why it
/// has none there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Console {
    /// Only under these Vagrant providers (empty: wherever the run goes). A machine's consoles
    /// name each provider once.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub providers: BTreeSet<Provider>,
    /// The protocol it speaks, or `none`.
    pub protocol: ConsoleProtocol,
    /// Where the runner finds it; absent for `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ConsoleSource>,
    /// The account it signs in as, when it signs in to the machine's own OS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<ConsoleAccount>,
    /// Why there is none, or what it needs (the extension pack VirtualBox's takes), in Isoloom's
    /// words. Required for `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Console {
    /// No console under `providers` (empty: anywhere), and why.
    #[must_use]
    pub fn none(providers: BTreeSet<Provider>, why: impl Into<String>) -> Console {
        Console {
            providers,
            protocol: ConsoleProtocol::None,
            source: None,
            account: None,
            note: Some(why.into()),
        }
    }

    /// A console speaking `protocol` from `source`, under `providers` (empty: anywhere).
    #[must_use]
    pub fn at(providers: BTreeSet<Provider>, protocol: ConsoleProtocol, source: ConsoleSource) -> Console {
        Console {
            providers,
            protocol,
            source: Some(source),
            account: None,
            note: None,
        }
    }

    /// Whether it applies under `provider` (`None`: a run with no provider, or one not chosen yet).
    #[must_use]
    pub fn applies_to(&self, provider: Option<Provider>) -> bool {
        self.providers.is_empty() || provider.is_some_and(|p| self.providers.contains(&p))
    }

    /// Why its fields don't hold together, if they don't: `none` with a source or without a
    /// note, a console with no source, a source speaking another protocol than the one said.
    pub(crate) fn shape_error(&self) -> Option<&'static str> {
        match (&self.protocol, &self.source) {
            (ConsoleProtocol::None, Some(_)) => Some("`none` has no `source`"),
            (ConsoleProtocol::None, None) if self.note.as_deref().is_none_or(str::is_empty) => Some("`none` says why in its `note`"),
            (ConsoleProtocol::None, None) => None,
            (_, None) => Some("a console says where it is in its `source`"),
            (p, Some(s)) if s.speaks().is_some_and(|spoken| spoken != *p) => Some("its `source` speaks another protocol"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_console_is_tagged_by_its_source() {
        let text = r#"{"providers":["libvirt"],"protocol":"vnc","source":{"from":"libvirt"}}"#;
        let c: Console = serde_json::from_str(text).unwrap();
        assert_eq!(c.source, Some(ConsoleSource::Libvirt {}));
        assert_eq!(serde_json::to_string(&c).unwrap(), text);
        let none = r#"{"protocol":"none","note":"UTM shows a VM's screen over SPICE only"}"#;
        let n: Console = serde_json::from_str(none).unwrap();
        assert!(n.applies_to(None) && n.applies_to(Some(Provider::Utm)), "no provider: everywhere");
        assert!(!c.applies_to(Some(Provider::Qemu)) && !c.applies_to(None));
        let tunnel = r#"{"protocol":"rdp","source":{"from":"ssh_tunnel","via":"${outputs.address_web}","user":"debian","host":"10.0.1.5","port":3389},"account":{"user":"isoloom","password":"${outputs.windows_password}"}}"#;
        let t: Console = serde_json::from_str(tunnel).unwrap();
        assert!(matches!(t.source, Some(ConsoleSource::SshTunnel { port: 3389, .. })));
        assert_eq!(serde_json::to_string(&t).unwrap(), tunnel);
        assert!(serde_json::from_str::<Console>(r#"{"protocol":"vnc","source":{"from":"spice"}}"#).is_err());
        assert!(serde_json::from_str::<Console>(r#"{"protocol":"vnc","source":{"from":"libvirt","port":5900}}"#).is_err());
    }

    #[test]
    fn a_console_holds_together() {
        let all = BTreeSet::new;
        assert_eq!(Console::none(all(), "no display").shape_error(), None);
        assert!(Console::none(all(), "").shape_error().is_some(), "none says why");
        assert_eq!(Console::at(all(), ConsoleProtocol::Vnc, ConsoleSource::Libvirt {}).shape_error(), None);
        assert!(Console::at(all(), ConsoleProtocol::Rdp, ConsoleSource::Libvirt {}).shape_error().is_some());
        assert!(Console::at(all(), ConsoleProtocol::Vnc, ConsoleSource::Vrde {}).shape_error().is_some());
        let mut with_source = Console::none(all(), "x");
        with_source.source = Some(ConsoleSource::Runner {});
        assert!(with_source.shape_error().is_some());
        let mut sourceless = Console::at(all(), ConsoleProtocol::Vnc, ConsoleSource::Runner {});
        sourceless.source = None;
        assert!(sourceless.shape_error().is_some());
        let tunnel = ConsoleSource::SshTunnel {
            via: Template::text("a"),
            user: Template::text("u"),
            host: Template::text("10.0.0.2"),
            port: 5900,
        };
        assert_eq!(Console::at(all(), ConsoleProtocol::Vnc, tunnel.clone()).shape_error(), None);
        assert_eq!(Console::at(all(), ConsoleProtocol::Rdp, tunnel).shape_error(), None);
    }
}
