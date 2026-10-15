//! Reaching a Windows machine to deliver its secrets. The receiving script and the values
//! travel on the connection's standard input; the command line is the same short bootstrap for
//! every machine (`isoloom_render::secrets::windows::powershell_args`).
//!
//! Two ways in, the ones the machine is already set up with:
//! - OpenSSH (`vm.image.winssh` on Vagrant, an existing Windows host): `ssh` or `vagrant ssh`
//!   runs `powershell` with the bootstrap, as a Linux machine's delivery does with `sh`.
//! - WinRM (the other Vagrant boxes, the clouds, Proxmox): `python3` with pywinrm, on this
//!   host, sends the input on WinRM's own input stream (the `Send` operation). Neither Vagrant
//!   (`vagrant winrm -c`) nor Terraform can feed a remote command's input, and the controller
//!   comes up after the machines it would deliver to, so the runner talks WinRM itself. The
//!   endpoint comes from `vagrant winrm-config`, or the address a Terraform set-up announces
//!   and the password Terraform keeps in its state; on Proxmox, through an SSH tunnel by the
//!   router, the only way in from outside.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use isoloom_render::secrets::windows::{command_line, powershell_args};

/// The script that talks WinRM (pywinrm), run by `python3 -c`.
const HELPER: &str = include_str!("winrm.py");
/// The WinRM port on a machine Isoloom sets up (HTTP).
const WINRM_PORT: u16 = 5985;

/// Where and as whom WinRM is reached.
#[derive(Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// `http(s)://<host>:<port>/wsman`.
    pub url: String,
    /// The account.
    pub user: String,
    /// Its password (the machine's, never a secret of the spec).
    pub password: String,
    /// NTLM instead of Basic.
    pub ntlm: bool,
}

/// A tunnel kept open while a delivery uses it, closed when dropped.
pub struct Tunnel(Child);

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The command delivering over WinRM at `e`: the helper, the account in its environment.
pub fn over_winrm(e: &Endpoint) -> Command {
    let mut c = crate::tools::process::command("python3");
    c.arg("-c").arg(HELPER).args(powershell_args());
    c.env("ISOLOOM_WINRM_URL", &e.url)
        .env("ISOLOOM_WINRM_USER", &e.user)
        .env("ISOLOOM_WINRM_PASSWORD", &e.password)
        .env("ISOLOOM_WINRM_AUTH", if e.ntlm { "ntlm" } else { "basic" });
    c
}

/// Whether this host can talk WinRM (python3 with pywinrm); what to install when it can't.
pub fn winrm_ready() -> Result<(), String> {
    let ok = crate::tools::process::command("python3")
        .args(["-c", "import winrm.protocol"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("a Windows machine gets its secrets over WinRM, which `isoloom run` talks through python3 and pywinrm: install them (`python3 -m pip install --user pywinrm`), then run again".into())
    }
}

/// `vagrant ssh` running the bootstrap on a Windows box reached over OpenSSH.
pub fn vagrant_ssh(dir: &Path, machine: &str) -> Command {
    let mut c = crate::tools::process::command("vagrant");
    c.current_dir(dir).args(["ssh", machine, "-c", &command_line(), "--", "-T"]);
    c
}

/// `ssh` running the bootstrap on an existing Windows host.
pub fn ssh(e: &isoloom_spec::model::External) -> Command {
    let mut c = crate::tools::process::command("ssh");
    c.args(["-o", "StrictHostKeyChecking=accept-new", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]);
    if let Some(k) = &e.key {
        c.arg("-i").arg(k);
    }
    if let Some(p) = e.port {
        c.arg(format!("-p{p}"));
    }
    c.arg(format!("{}@{}", e.user.as_deref().unwrap_or("Administrator"), e.address));
    c.arg(command_line());
    c
}

/// A Vagrant box's WinRM endpoint, from `vagrant winrm-config` (HTTPS and NTLM when its
/// image says `winrm: ssl`).
pub fn vagrant_endpoint(dir: &Path, machine: &str, ssl: bool) -> Result<Endpoint, String> {
    let out = crate::tools::process::command("vagrant")
        .current_dir(dir)
        .args(["winrm-config", machine])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("it isn't up yet".into());
    }
    parse_winrm_config(&String::from_utf8_lossy(&out.stdout), ssl).ok_or_else(|| "`vagrant winrm-config` gave no endpoint".into())
}

/// `vagrant winrm-config`'s `HostName`, `Port`, `User` and `Password`.
fn parse_winrm_config(text: &str, ssl: bool) -> Option<Endpoint> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key).and_then(|v| v.strip_prefix(' ')).map(|v| v.trim().to_string()))
    };
    let scheme = if ssl { "https" } else { "http" };
    Some(Endpoint {
        url: format!("{scheme}://{}:{}/wsman", field("HostName")?, field("Port")?),
        user: field("User")?,
        password: field("Password")?,
        ntlm: ssl,
    })
}

/// The endpoint a cloud's Terraform set-up announced in `line` (`isoloom@<address> winrm`, or
/// `winrm-ntlm`), with the password from the state of `module`. `None` for an SSH announcement.
pub fn announced(line: &str, module: &Path) -> Option<Result<Endpoint, String>> {
    let mut words = line.split_whitespace();
    let (user, host) = words.next()?.split_once('@')?;
    let ntlm = match words.next()? {
        "winrm" => false,
        "winrm-ntlm" => true,
        _ => return None,
    };
    Some(state_password(module).map(|password| Endpoint {
        url: format!("http://{host}:{WINRM_PORT}/wsman"),
        user: user.to_string(),
        password,
        ntlm,
    }))
}

/// The Windows machines' password (`random_password.windows`) from the module's state, which
/// Terraform writes as it creates each resource: there before any machine's set-up runs.
fn state_password(module: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(module.join("terraform.tfstate")).map_err(|_| "its password isn't known yet".to_string())?;
    let state: serde_json::Value = serde_json::from_str(&text).map_err(|_| "its password isn't known yet".to_string())?;
    state["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["type"] == "random_password" && r["name"] == "windows")
        .and_then(|r| r["instances"][0]["attributes"]["result"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "its password isn't known yet".into())
}

/// A Proxmox or ESXi Windows machine's endpoint: WinRM at `host`, reached through an SSH tunnel by the
/// router (`isoloom@<router>`), as `isoloom` with the module's password.
pub fn through_router(module: &Path, router: &str, host: &str) -> Result<(Endpoint, Tunnel), String> {
    let password = state_password(module)?;
    let port = TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
    let child = crate::tools::process::command("ssh")
        .args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "BatchMode=yes",
        ])
        .arg("-L")
        .arg(format!("127.0.0.1:{port}:{host}:{WINRM_PORT}"))
        .arg(format!("isoloom@{router}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let tunnel = Tunnel(child);
    let at = SocketAddr::from(([127, 0, 0, 1], port));
    let end = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect_timeout(&at, Duration::from_secs(1)).is_err() {
        if Instant::now() >= end {
            return Err("the tunnel through the router didn't open".into());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let endpoint = Endpoint {
        url: format!("http://127.0.0.1:{port}/wsman"),
        user: "isoloom".into(),
        password,
        ntlm: false,
    };
    Ok((endpoint, tunnel))
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, announced, parse_winrm_config};

    #[test]
    fn a_vagrant_endpoint_comes_from_winrm_config() {
        let text = "Host win\n  HostName 127.0.0.1\n  User vagrant\n  Password vagrant\n  Port 55985\n  RDPHostName 127.0.0.1\n  RDPPort 53389\n";
        assert_eq!(
            parse_winrm_config(text, false),
            Some(Endpoint {
                url: "http://127.0.0.1:55985/wsman".into(),
                user: "vagrant".into(),
                password: "vagrant".into(),
                ntlm: false,
            })
        );
        assert!(parse_winrm_config(text, true).is_some_and(|e| e.url.starts_with("https://") && e.ntlm));
        assert!(parse_winrm_config("Host win\n", false).is_none());
    }

    #[test]
    fn a_cloud_endpoint_is_announced_and_its_password_read_from_the_state() {
        let dir = std::env::temp_dir().join(format!("isoloom-winrm-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(announced("admin@1.2.3.4", &dir).is_none(), "SSH: not WinRM");
        assert_eq!(announced("isoloom@1.2.3.4 winrm", &dir).unwrap().unwrap_err(), "its password isn't known yet");
        let state = r#"{"resources":[{"type":"random_password","name":"windows","instances":[{"attributes":{"result":"pw"}}]}]}"#;
        std::fs::write(dir.join("terraform.tfstate"), state).unwrap();
        let e = announced("isoloom@1.2.3.4 winrm-ntlm", &dir).unwrap().unwrap();
        assert_eq!((e.url.as_str(), e.password.as_str(), e.ntlm), ("http://1.2.3.4:5985/wsman", "pw", true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_winrm_command_has_no_secret_in_its_line() {
        let e = Endpoint {
            url: "http://h:5985/wsman".into(),
            user: "u".into(),
            password: "p".into(),
            ntlm: false,
        };
        let c = super::over_winrm(&e);
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"-EncodedCommand".to_string()) && !args.iter().any(|a| a == "p"), "{args:?}");
        assert!(c.get_envs().any(|(k, v)| k == "ISOLOOM_WINRM_AUTH" && v == Some("basic".as_ref())));
    }
}
