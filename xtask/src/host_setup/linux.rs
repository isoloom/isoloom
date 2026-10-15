//! The Linux host's tools, without root, in `~/.local/bin`: pinned versions, every download
//! checked against the SHA-256 its publisher lists. Rust itself comes first, by hand
//! (<https://rustup.rs>), since this task runs on it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

use crate::check_all::TAURI_PACKAGES;
use crate::{Res, process, repo};

const CARGO_MACHETE: &str = "0.9.2";
const CARGO_NEXTEST: &str = "0.9.148";
const TERRAFORM: &str = "1.16.6";
const KUBECTL: &str = "1.37.1";
const KIND: &str = "0.33.0";
const HELM: &str = "4.3.0";
const PWSH: &str = "7.6.7";

/// Where the tools go and the search path that finds them.
struct Ctx {
    bin: PathBuf,
    share: PathBuf,
    /// A private folder for the downloads, removed at the end.
    work: PathBuf,
    path: OsString,
}

impl Ctx {
    fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.env("PATH", &self.path);
        c
    }

    /// The first line `command` prints, when it runs.
    fn found(&self, command: &[&str]) -> Option<String> {
        let (program, args) = command.split_first()?;
        process::first_line(self.cmd(program).args(args))
    }
}

/// One tool: how to see it is there (the line found must hold `wants`), and how to install it.
struct Tool {
    name: String,
    probe: fn(&Ctx) -> Option<String>,
    wants: String,
    install: fn(&Ctx) -> Res<()>,
}

fn tools() -> Vec<Tool> {
    let tool = |name: String, probe, wants: String, install| Tool { name, probe, wants, install };
    vec![
        tool("Rust (rustfmt, clippy)".into(), probe_rust, "rustc ".into(), install_rust_components),
        tool(
            format!("cargo-machete {CARGO_MACHETE}"),
            |c| c.found(&["cargo", "machete", "--version"]),
            CARGO_MACHETE.into(),
            install_machete,
        ),
        tool(
            format!("cargo-nextest {CARGO_NEXTEST}"),
            |c| c.found(&["cargo", "nextest", "--version"]),
            CARGO_NEXTEST.into(),
            install_nextest,
        ),
        tool(
            format!("Terraform {TERRAFORM}"),
            |c| c.found(&["terraform", "version"]),
            format!("v{TERRAFORM}"),
            install_terraform,
        ),
        tool(
            format!("kubectl {KUBECTL}"),
            |c| c.found(&["kubectl", "version", "--client"]),
            format!("v{KUBECTL}"),
            install_kubectl,
        ),
        tool(format!("kind {KIND}"), |c| c.found(&["kind", "version"]), format!("v{KIND}"), install_kind),
        tool(
            format!("Helm {HELM}"),
            |c| c.found(&["helm", "version", "--short"]),
            format!("v{HELM}"),
            install_helm,
        ),
        tool(format!("PowerShell {PWSH}"), |c| c.found(&["pwsh", "--version"]), PWSH.into(), install_pwsh),
        tool("Ansible and ansible-lint".into(), probe_ansible, "ansible-lint".into(), install_ansible),
    ]
}

/// Checks each tool and installs the missing ones (unless `check`), then says what needs root.
pub(super) fn run(check: bool) -> Res<ExitCode> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME isn't set")?);
    let user_path = std::env::var_os("PATH").unwrap_or_default();
    let (bin, cargo_bin) = (home.join(".local/bin"), home.join(".cargo/bin"));
    let mut dirs = vec![bin.clone(), cargo_bin];
    dirs.extend(std::env::split_paths(&user_path));
    let ctx = Ctx {
        bin: bin.clone(),
        share: home.join(".local/share"),
        work: std::env::temp_dir().join(format!("isoloom-host-setup-{}", std::process::id())),
        path: std::env::join_paths(dirs)?,
    };
    std::fs::create_dir_all(&ctx.bin)?;
    std::fs::create_dir_all(&ctx.share)?;
    std::fs::create_dir_all(&ctx.work)?;
    // Every tool is reported, so no short-circuit: `all` would stop at the first missing one.
    let results: Vec<bool> = tools().iter().map(|t| ensure(&ctx, t, check)).collect();
    let ok = results.iter().all(|r| *r);
    // The downloads go; one left behind in the temporary folder is harmless.
    repo::remove(&ctx.work).ok();
    root_notes(&ctx);
    if !std::env::split_paths(&user_path).any(|d| d == bin) {
        println!(
            "note      {} and ~/.cargo/bin are not on PATH here; add them for your shells (check-all adds them)",
            bin.display()
        );
    }
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Reports a tool, installing it when it is missing; whether it is there in the end.
fn ensure(ctx: &Ctx, t: &Tool, check: bool) -> bool {
    let found = (t.probe)(ctx);
    match found {
        Some(line) if line.contains(&t.wants) => {
            println!("ok        {} ({line})", t.name);
            return true;
        }
        _ if check => {
            println!("missing   {}{}", t.name, found.map(|f| format!(" (found: {f})")).unwrap_or_default());
            return false;
        }
        _ => println!("install   {}", t.name),
    }
    match (t.install)(ctx) {
        Ok(()) => {
            println!("          {}", (t.probe)(ctx).unwrap_or_default());
            true
        }
        Err(e) => {
            println!("failed    {}: {e}", t.name);
            false
        }
    }
}

/// What needs root: printed, not done.
fn root_notes(ctx: &Ctx) {
    // PowerShell (.NET) needs ICU.
    let icu = process::output(ctx.cmd("/sbin/ldconfig").arg("-p")).is_ok_and(|o| o.contains("libicuuc"));
    if !icu {
        println!("root      ICU for PowerShell: sudo apt install libicu76 (Debian 13)");
    }
    if ctx.found(&["pkg-config", "--exists", "webkit2gtk-4.1"]).is_some() {
        println!("ok        Tauri system packages (the launcher)");
    } else {
        println!("root      Tauri system packages (the launcher): sudo apt install build-essential pkg-config {TAURI_PACKAGES}");
    }
}

fn probe_rust(ctx: &Ctx) -> Option<String> {
    let installed = process::output(ctx.cmd("rustup").args(["component", "list", "--installed"])).ok()?;
    (installed.contains("rustfmt") && installed.contains("clippy")).then(|| ctx.found(&["rustc", "--version"]))?
}

fn probe_ansible(ctx: &Ctx) -> Option<String> {
    ctx.found(&["ansible-playbook", "--version"])?;
    ctx.found(&["ansible-lint", "--version"])
}

fn install_rust_components(ctx: &Ctx) -> Res<()> {
    process::run(ctx.cmd("rustup").args(["component", "add", "rustfmt", "clippy"]))
}

fn install_machete(ctx: &Ctx) -> Res<()> {
    process::run(ctx.cmd("cargo").args(["install", "--locked", &format!("cargo-machete@{CARGO_MACHETE}")]))
}

fn install_nextest(ctx: &Ctx) -> Res<()> {
    process::run(ctx.cmd("cargo").args(["install", "--locked", &format!("cargo-nextest@{CARGO_NEXTEST}")]))
}

/// Downloads `url` into the work folder as `name` and checks it against the SHA-256 listed for
/// `name` (or the only hash) in `sums`.
fn fetch_verified(ctx: &Ctx, url: &str, sums: &str, name: &str) -> Res<PathBuf> {
    let file = ctx.work.join(name);
    process::quiet(ctx.cmd("curl").args(["-fsSL", url, "-o"]).arg(&file))?;
    let listed = ctx.cmd("curl").args(["-fsSL", sums]).output()?;
    let expected = expected_sum(&listed.stdout, name).ok_or_else(|| format!("no SHA-256 for {name} in {sums}"))?;
    let actual = crate::text::hex(&Sha256::digest(std::fs::read(&file)?));
    if expected != actual {
        return Err(format!("checksum mismatch for {name}: expected {expected}, got {actual}").into());
    }
    Ok(file)
}

/// The SHA-256 a checksum list gives for `name` (`<hash> <name>`, `<hash> *<name>`, or a lone
/// hash). Some lists are UTF-16 with a byte order mark (PowerShell's): the text is kept, the
/// rest dropped.
fn expected_sum(listed: &[u8], name: &str) -> Option<String> {
    let text: String = listed.iter().filter(|b| **b != 0 && **b != b'\r').map(|b| char::from(*b)).collect();
    let text = text.trim_start_matches(|c: char| !c.is_ascii_hexdigit());
    text.lines().find_map(|line| match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        [hash] => Some(hash.to_ascii_lowercase()),
        [hash, file] if *file == name || file.strip_prefix('*') == Some(name) => Some(hash.to_ascii_lowercase()),
        _ => None,
    })
}

/// Puts `file` in the bin folder as `name`, executable.
fn install_bin(ctx: &Ctx, file: &Path, name: &str) -> Res<()> {
    process::run(ctx.cmd("install").args(["-m", "0755"]).arg(file).arg(ctx.bin.join(name)))
}

fn install_terraform(ctx: &Ctx) -> Res<()> {
    let name = format!("terraform_{TERRAFORM}_linux_amd64.zip");
    let base = format!("https://releases.hashicorp.com/terraform/{TERRAFORM}");
    let zip = fetch_verified(ctx, &format!("{base}/{name}"), &format!("{base}/terraform_{TERRAFORM}_SHA256SUMS"), &name)?;
    process::run(ctx.cmd("unzip").args(["-o", "-q"]).arg(zip).arg("terraform").arg("-d").arg(&ctx.bin))
}

fn install_kubectl(ctx: &Ctx) -> Res<()> {
    let url = format!("https://dl.k8s.io/release/v{KUBECTL}/bin/linux/amd64/kubectl");
    let file = fetch_verified(ctx, &url, &format!("{url}.sha256"), "kubectl")?;
    install_bin(ctx, &file, "kubectl")
}

fn install_kind(ctx: &Ctx) -> Res<()> {
    let url = format!("https://github.com/kubernetes-sigs/kind/releases/download/v{KIND}/kind-linux-amd64");
    let file = fetch_verified(ctx, &url, &format!("{url}.sha256sum"), "kind-linux-amd64")?;
    install_bin(ctx, &file, "kind")
}

fn install_helm(ctx: &Ctx) -> Res<()> {
    let name = format!("helm-v{HELM}-linux-amd64.tar.gz");
    let url = format!("https://get.helm.sh/{name}");
    let archive = fetch_verified(ctx, &url, &format!("{url}.sha256sum"), &name)?;
    process::run(ctx.cmd("tar").arg("-xzf").arg(archive).arg("-C").arg(&ctx.work).arg("linux-amd64/helm"))?;
    install_bin(ctx, &ctx.work.join("linux-amd64/helm"), "helm")
}

fn install_pwsh(ctx: &Ctx) -> Res<()> {
    let name = format!("powershell-{PWSH}-linux-x64.tar.gz");
    let base = format!("https://github.com/PowerShell/PowerShell/releases/download/v{PWSH}");
    let archive = fetch_verified(ctx, &format!("{base}/{name}"), &format!("{base}/hashes.sha256"), &name)?;
    let dir = ctx.share.join("powershell").join(PWSH);
    repo::remove(&dir)?;
    std::fs::create_dir_all(&dir)?;
    process::run(ctx.cmd("tar").arg("-xzf").arg(archive).arg("-C").arg(&dir))?;
    process::run(ctx.cmd("chmod").arg("+x").arg(dir.join("pwsh")))?;
    process::run(ctx.cmd("ln").arg("-sf").arg(dir.join("pwsh")).arg(ctx.bin.join("pwsh")))
}

fn install_ansible(ctx: &Ctx) -> Res<()> {
    if ctx.found(&["pipx", "--version"]).is_none() {
        return Err("pipx is missing (as root: apt install pipx)".into());
    }
    process::run(ctx.cmd("pipx").args(["install", "--include-deps", "ansible"]))?;
    process::run(ctx.cmd("pipx").args(["install", "ansible-lint"]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sum_is_found_by_name_or_alone() {
        let list = b"aa11  other.zip\nBB22 *terraform.zip\n";
        assert_eq!(expected_sum(list, "terraform.zip").as_deref(), Some("bb22"));
        assert_eq!(expected_sum(b"cc33\n", "kubectl").as_deref(), Some("cc33"));
        assert_eq!(expected_sum(b"dd44  x\n", "y"), None);
    }

    #[test]
    fn a_utf16_list_with_a_byte_order_mark_reads_as_text() {
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("ee55  pwsh.tar.gz\r\n".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(expected_sum(&utf16, "pwsh.tar.gz").as_deref(), Some("ee55"));
    }
}
