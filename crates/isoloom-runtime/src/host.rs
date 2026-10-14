//! Host readiness: whether *this machine* can run a target. `isoloom targets` says where a spec
//! can run; this says which of those the host is equipped for, and what is missing: Docker and
//! Compose v2, Vagrant and a provider, a reachable Kubernetes context, Terraform and a cloud's
//! credentials, Proxmox's endpoint and token. Nothing here touches the spec; tools embedding
//! Isoloom call [`check`] for their runtime pickers.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::Target;

/// What a target needs from the host, and whether it is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub target: Target,
    pub cloud: Option<String>,
    pub ready: bool,
    /// What was found, and what is missing (each a short line).
    pub notes: Vec<String>,
}

impl Readiness {
    /// One line: `ready (Docker 27.3.1, Compose v2.29.7)` or `not ready: vagrant isn't installed`.
    pub fn summary(&self) -> String {
        let (found, missing): (Vec<&String>, Vec<&String>) = self.notes.iter().partition(|n| !n.starts_with('!'));
        if self.ready {
            format!("ready ({})", found.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
        } else {
            format!(
                "not ready: {}",
                missing.iter().map(|s| s.trim_start_matches('!').trim()).collect::<Vec<_>>().join("; ")
            )
        }
    }
}

/// How the probes see the host: run a tool, read an environment variable, test a path.
/// Replaceable for tests.
pub struct Host<'a> {
    pub run: &'a dyn Fn(&str, &[&str]) -> Option<String>,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub exists: &'a dyn Fn(&Path) -> bool,
    /// Whether a path opens for reading and writing (a device such as `/dev/kvm`).
    pub opens: &'a dyn Fn(&Path) -> bool,
    pub home: PathBuf,
}

/// Runs a tool and returns its stdout when it succeeds, giving up (and killing it) after
/// `secs`: a wedged hypervisor service must not hang `status` or `doctor`.
pub fn run_limited(program: &str, args: &[&str], secs: u64) -> Result<Option<String>, crate::HostError> {
    use std::process::Stdio;
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    // On Windows a console program started by a windowed app (a desktop app, an IDE) opens a
    // console window of its own: every probe would flash one on screen. Output is piped anyway.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(crate::HostError::NotInstalled(program.to_string())),
        Err(e) => return Err(e.into()),
    };
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(s) = stdout.as_mut() {
            use std::io::Read;
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().unwrap_or_default();
                return Ok(status.success().then(|| out.trim().to_string()));
            }
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::HostError::Timeout {
                    program: program.to_string(),
                    secs,
                });
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// The real host.
#[allow(clippy::type_complexity)]
pub fn real_host() -> (
    impl Fn(&str, &[&str]) -> Option<String>,
    impl Fn(&str) -> Option<String>,
    impl Fn(&Path) -> bool,
    impl Fn(&Path) -> bool,
    PathBuf,
) {
    let run = |program: &str, args: &[&str]| -> Option<String> { run_limited(program, args, 20).ok().flatten() };
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let exists = |p: &Path| p.exists();
    let opens = |p: &Path| std::fs::OpenOptions::new().read(true).write(true).open(p).is_ok();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    (run, env, exists, opens, home)
}

/// Checks a target against the real host.
pub fn check(target: Target, cloud: Option<&str>) -> Readiness {
    with_real_host(|host| check_with(target, cloud, host))
}

/// Every target (each cloud for the cloud targets) against the real host.
pub fn all() -> Vec<Readiness> {
    with_real_host(|host| {
        let mut out = Vec::new();
        for t in Target::ALL {
            match t {
                // A cloud-services environment names its own cloud: `doctor` checks the three.
                Target::CloudServices => {
                    for c in ["aws", "azure", "gcp"] {
                        out.push(check_with(t, Some(c), host));
                    }
                }
                Target::CloudDocker | Target::CloudVm => {
                    for c in CLOUDS {
                        out.push(check_with(t, Some(c), host));
                    }
                }
                _ => out.push(check_with(t, None, host)),
            }
        }
        out
    })
}

/// Runs `f` with the real host.
fn with_real_host<R>(f: impl FnOnce(&Host) -> R) -> R {
    let (run, env, exists, opens, home) = real_host();
    f(&Host {
        run: &run,
        env: &env,
        exists: &exists,
        opens: &opens,
        home,
    })
}

pub const CLOUDS: &[&str] = &["aws", "azure", "gcp", "digitalocean", "linode", "oci"];

/// Why KVM can't be used here, on Linux: no `/dev/kvm` (virtualization off, or a VM without
/// nested virtualization), or the user can't open it. None elsewhere (no such device to probe).
fn kvm_problem(h: &Host) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let dev = Path::new("/dev/kvm");
    if !(h.exists)(dev) {
        return Some("no /dev/kvm (turn on virtualization in the firmware, or nested virtualization in a VM)".into());
    }
    if !(h.opens)(dev) {
        return Some("/dev/kvm isn't usable by this user (add it to the kvm and libvirt groups, then sign in again)".into());
    }
    None
}

/// What was found on the host, and whether anything needed is missing.
struct Notes {
    lines: Vec<String>,
    ok: bool,
}

impl Notes {
    fn new() -> Notes {
        Notes { lines: Vec::new(), ok: true }
    }

    /// Notes what was found, or (marked with `!`) what is missing and makes the target not ready.
    fn need(&mut self, found: Option<String>, missing: &str) {
        if let Some(f) = found {
            self.lines.push(f);
        } else {
            self.lines.push(format!("! {missing}"));
            self.ok = false;
        }
    }
}

/// Checks a target against a host.
pub fn check_with(target: Target, cloud: Option<&str>, h: &Host) -> Readiness {
    let mut notes = Notes::new();
    match target {
        Target::Docker | Target::Hosted => docker(h, &mut notes),
        Target::Vagrant | Target::DockerVm | Target::Hybrid => vagrant(target, h, &mut notes),
        Target::Kubernetes => kubernetes(h, &mut notes),
        Target::External => {
            notes.need((h.run)("ssh", &["-G", "localhost"]).map(|_| "ssh".to_string()), "ssh isn't installed");
        }
        Target::Proxmox => {
            terraform(h, &mut notes);
            proxmox(h, &mut notes);
        }
        Target::CloudDocker | Target::CloudVm | Target::CloudServices => {
            terraform(h, &mut notes);
            let (found, missing) = cloud_credentials(cloud.unwrap_or("aws"), h);
            notes.need(found, &missing);
        }
    }
    Readiness {
        target,
        cloud: cloud.map(str::to_string),
        ready: notes.ok,
        notes: notes.lines,
    }
}

fn docker(h: &Host, notes: &mut Notes) {
    notes.need(
        (h.run)("docker", &["version", "--format", "{{.Server.Version}}"]).map(|v| format!("Docker {v}")),
        "docker isn't installed or its daemon isn't running",
    );
    notes.need(
        (h.run)("docker", &["compose", "version", "--short"])
            .filter(|v| !v.starts_with('1'))
            .map(|v| format!("Compose {v}")),
        "docker compose v2 isn't available",
    );
}

fn vagrant(target: Target, h: &Host, notes: &mut Notes) {
    notes.need(
        (h.run)("vagrant", &["--version"]),
        "vagrant isn't installed (https://developer.hashicorp.com/vagrant/install)",
    );
    let (providers, unusable) = vagrant_providers(h);
    if target == Target::Hybrid {
        notes.need(
            providers.iter().find(|p| p.starts_with("VirtualBox")).cloned(),
            "hybrid needs VirtualBox (promiscuous interfaces are set per provider; VirtualBox only for now)",
        );
    } else {
        notes.need(
            (!providers.is_empty()).then(|| format!("providers: {}", providers.join(", "))),
            &unusable.unwrap_or_else(|| "no Vagrant provider found (VirtualBox, VMware, Parallels, libvirt, UTM, QEMU or ESXi)".to_string()),
        );
    }
}

/// The Vagrant providers found (each with its version where known), and why libvirt can't be
/// used when its plugin is there but KVM isn't.
fn vagrant_providers(h: &Host) -> (Vec<String>, Option<String>) {
    let plugins = (h.run)("vagrant", &["plugin", "list"]).unwrap_or_default();
    let mut providers = Vec::new();
    if let Some(v) = (h.run)("VBoxManage", &["--version"]) {
        providers.push(format!("VirtualBox {v}"));
    }
    if (h.run)("vmrun", &["-T", "ws", "list"]).is_some() || plugins.contains("vagrant-vmware-desktop") {
        providers.push("VMware".to_string());
    }
    if (h.run)("prlctl", &["--version"]).is_some() {
        providers.push("Parallels".to_string());
    }
    // libvirt runs its VMs on KVM: with the plugin and virsh but no usable /dev/kvm,
    // `vagrant up` fails at boot, so it doesn't count (and the reason is given instead).
    let mut unusable = None;
    if plugins.contains("vagrant-libvirt") && (h.run)("virsh", &["--version"]).is_some() {
        match kvm_problem(h) {
            None => providers.push("libvirt".to_string()),
            Some(why) => unusable = Some(format!("libvirt can't run: {why}")),
        }
    }
    for (plugin, provider) in [("vagrant_utm", "UTM"), ("vagrant-qemu", "QEMU"), ("vagrant-vmware-esxi", "ESXi")] {
        if plugins.contains(plugin) {
            providers.push(provider.to_string());
        }
    }
    (providers, unusable)
}

fn kubernetes(h: &Host, notes: &mut Notes) {
    notes.need(
        (h.run)("kubectl", &["version", "--client", "-o", "yaml"]).map(|_| "kubectl".to_string()),
        "kubectl isn't installed",
    );
    notes.need(
        (h.run)("kubectl", &["config", "current-context"]).map(|c| format!("context {c}")),
        "no current kubectl context",
    );
    notes.need(
        (h.run)("kubectl", &["cluster-info", "--request-timeout=5s"]).map(|_| "cluster reachable".to_string()),
        "the cluster doesn't answer (kubectl cluster-info)",
    );
}

fn terraform(h: &Host, notes: &mut Notes) {
    notes.need(
        (h.run)("terraform", &["version", "-json"]).map(|_| "Terraform".to_string()),
        "terraform isn't installed",
    );
}

fn proxmox(h: &Host, notes: &mut Notes) {
    notes.need((h.env)("PROXMOX_VE_ENDPOINT").map(|e| format!("endpoint {e}")), "PROXMOX_VE_ENDPOINT isn't set");
    notes.need(
        (h.env)("PROXMOX_VE_API_TOKEN")
            .map(|_| "API token".to_string())
            .or_else(|| (h.env)("PROXMOX_VE_USERNAME").map(|_| "username/password".to_string())),
        "no Proxmox credentials (PROXMOX_VE_API_TOKEN, or PROXMOX_VE_USERNAME and PROXMOX_VE_PASSWORD)",
    );
}

/// A cloud's credentials as found on the host, and what to say when they aren't.
fn cloud_credentials(cloud: &str, h: &Host) -> (Option<String>, String) {
    let (found, missing) = match cloud {
        "aws" => (
            (h.env)("AWS_ACCESS_KEY_ID")
                .map(|_| "AWS keys in the environment".to_string())
                .or_else(|| (h.env)("AWS_PROFILE").map(|p| format!("AWS profile {p}")))
                .or_else(|| (h.exists)(&h.home.join(".aws/credentials")).then(|| "~/.aws/credentials".to_string()))
                // `aws login` and SSO keep their session under ~/.aws/login or ~/.aws/sso.
                .or_else(|| (h.exists)(&h.home.join(".aws/config")).then(|| "~/.aws/config (aws login or SSO)".to_string())),
            "no AWS credentials (AWS_ACCESS_KEY_ID, AWS_PROFILE or ~/.aws/credentials; `aws login`)",
        ),
        "azure" => (
            (h.env)("ARM_CLIENT_ID")
                .map(|_| "Azure service principal in the environment".to_string())
                .or_else(|| (h.run)("az", &["account", "show", "--query", "name", "-o", "tsv"]).map(|n| format!("Azure account {n}"))),
            "not logged in to Azure (`az login`, or ARM_CLIENT_ID/ARM_CLIENT_SECRET/ARM_TENANT_ID)",
        ),
        "gcp" => (
            (h.env)("GOOGLE_APPLICATION_CREDENTIALS")
                .map(|_| "GOOGLE_APPLICATION_CREDENTIALS".to_string())
                .or_else(|| {
                    (h.exists)(&h.home.join(".config/gcloud/application_default_credentials.json"))
                        .then(|| "gcloud application default credentials".to_string())
                }),
            "no Google Cloud credentials (`gcloud auth application-default login`)",
        ),
        "digitalocean" => (
            (h.env)("DIGITALOCEAN_TOKEN").map(|_| "DIGITALOCEAN_TOKEN".to_string()),
            "DIGITALOCEAN_TOKEN isn't set",
        ),
        "linode" => ((h.env)("LINODE_TOKEN").map(|_| "LINODE_TOKEN".to_string()), "LINODE_TOKEN isn't set"),
        "oci" => (
            (h.exists)(&h.home.join(".oci/config"))
                .then(|| "~/.oci/config".to_string())
                .or_else(|| (h.env)("OCI_CLI_USER").map(|_| "OCI environment".to_string())),
            "no Oracle Cloud configuration (~/.oci/config)",
        ),
        other => return (None, format!("unknown cloud `{other}` (aws, azure, gcp, digitalocean, linode, oci)")),
    };
    (found, missing.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks a target on a host where only `tools` run and only `envs` are set.
    fn check_on(target: Target, cloud: Option<&str>, tools: &[&str], envs: &[(&str, &str)]) -> Readiness {
        let run = |p: &str, _: &[&str]| tools.contains(&p).then(|| format!("{p} 1.0"));
        let env = |k: &str| envs.iter().find(|(e, _)| *e == k).map(|(_, v)| v.to_string());
        let exists = |_: &Path| false;
        check_with(
            target,
            cloud,
            &Host {
                run: &run,
                env: &env,
                exists: &exists,
                opens: &exists,
                home: PathBuf::new(),
            },
        )
    }

    #[test]
    fn docker_needs_the_daemon_and_compose() {
        let r = check_on(Target::Docker, None, &["docker"], &[]);
        assert!(r.ready, "{r:?}");
        let r = check_on(Target::Docker, None, &[], &[]);
        assert!(!r.ready && r.summary().contains("docker isn't installed"));
    }

    #[test]
    fn vagrant_needs_a_provider_and_hybrid_virtualbox() {
        let r = check_on(Target::Vagrant, None, &["vagrant"], &[]);
        assert!(!r.ready && r.summary().contains("no Vagrant provider"));
        let r = check_on(Target::Vagrant, None, &["vagrant", "prlctl"], &[]);
        assert!(r.ready && r.summary().contains("Parallels"));
        let r = check_on(Target::Hybrid, None, &["vagrant", "prlctl"], &[]);
        assert!(!r.ready && r.summary().contains("VirtualBox"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn libvirt_counts_only_with_a_usable_kvm() {
        let run = |p: &str, a: &[&str]| match (p, a.first().copied()) {
            ("vagrant", Some("plugin")) => Some("vagrant-libvirt (0.12.2, global)".to_string()),
            ("vagrant" | "virsh", _) => Some(format!("{p} 1.0")),
            _ => None,
        };
        let env = |_: &str| None;
        let probe = |kvm: bool, usable: bool| {
            let exists = move |_: &Path| kvm;
            let opens = move |_: &Path| usable;
            check_with(
                Target::Vagrant,
                None,
                &Host {
                    run: &run,
                    env: &env,
                    exists: &exists,
                    opens: &opens,
                    home: PathBuf::new(),
                },
            )
        };
        let r = probe(true, true);
        assert!(r.ready && r.summary().contains("libvirt"), "{r:?}");
        let r = probe(false, false);
        assert!(!r.ready && r.summary().contains("no /dev/kvm"), "{r:?}");
        let r = probe(true, false);
        assert!(!r.ready && r.summary().contains("kvm and libvirt groups"), "{r:?}");
    }

    #[test]
    fn clouds_need_terraform_and_their_credentials() {
        let (tools, envs) = (&["terraform"][..], &[("DIGITALOCEAN_TOKEN", "t")][..]);
        assert!(check_on(Target::CloudVm, Some("digitalocean"), tools, envs).ready);
        let aws = check_on(Target::CloudDocker, Some("aws"), tools, envs);
        assert!(!aws.ready && aws.summary().contains("no AWS credentials"));
        assert!(!check_on(Target::Proxmox, None, tools, envs).ready);
    }
}
