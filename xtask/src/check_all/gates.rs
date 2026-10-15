//! A host's part: the gates of the selected repositories, run here, each reported to a
//! [`Sink`]. The same code runs on every host: this Mac (the full profile), and the copies on
//! the Linux and Windows hosts and in the container (the platform profile).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::collect::Sink;
use super::{Dirs, GateResult, Host, Profile, Repo, Status};
use crate::process;

/// One gate: its name and the command it runs in the repository's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Gate {
    /// Its name in the table.
    pub name: &'static str,
    /// The program and its arguments.
    pub argv: Vec<&'static str>,
    /// Variables set for it.
    pub env: Vec<(&'static str, &'static str)>,
}

fn gate(name: &'static str, argv: &[&'static str]) -> Gate {
    Gate {
        name,
        argv: argv.to_vec(),
        env: Vec::new(),
    }
}

/// Isoloom's gates: every one on the full profile, the tests on the others. Clippy runs without
/// `-D warnings`: a lint at deny fails it, and the lints still at warn are counted by the
/// ratchet (`cargo xtask quality`), which fails when a count goes up.
pub(super) fn isoloom(profile: Profile) -> Vec<Gate> {
    let mut gates = Vec::new();
    if profile == Profile::Full {
        gates.push(gate("fmt", &["cargo", "fmt", "--all", "--check"]));
        gates.push(gate("typos", &["typos"]));
        gates.push(gate("machete", &["cargo", "machete"]));
        let clippy = ["cargo", "clippy", "--workspace", "--all-targets"];
        gates.push(gate("clippy", &clippy));
        gates.push(gate("clippy-linux", &[&clippy[..], &["--target", "x86_64-unknown-linux-gnu"]].concat()));
        gates.push(gate("clippy-windows", &[&clippy[..], &["--target", "x86_64-pc-windows-msvc"]].concat()));
        let mut doc = gate("doc", &["cargo", "doc", "--workspace", "--no-deps"]);
        doc.env.push(("RUSTDOCFLAGS", "-Dwarnings"));
        gates.push(doc);
        gates.push(gate("quality", &["cargo", "xtask", "quality"]));
    }
    gates.push(gate("test", &["cargo", "test", "--workspace", "--no-fail-fast"]));
    gates
}

/// Isothrum's libraries (its root workspace; the desktop app is its own and moves out).
pub(super) fn isothrum(profile: Profile) -> Vec<Gate> {
    let mut gates = Vec::new();
    if profile == Profile::Full {
        gates.push(gate("fmt", &["cargo", "fmt", "--all", "--check"]));
        gates.push(gate("clippy", &["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]));
    }
    gates.push(gate("test", &["cargo", "test", "--workspace", "--no-fail-fast"]));
    gates
}

/// How this host runs its gates: its search path, and where each repository builds.
pub(super) struct Leg<'a> {
    host: Host,
    profile: Profile,
    dirs: &'a Dirs,
    path: OsString,
    /// The parent of each repository's build folder, when there is one.
    target_root: Option<PathBuf>,
}

/// This host's part: `profile`'s gates of the checkouts in `dirs`, each repository building
/// under `target_root` (else under `CARGO_TARGET_DIR`, else in its own `target`).
pub(super) fn leg<'a>(host: Host, profile: Profile, dirs: &'a Dirs, target_root: Option<&Path>) -> Leg<'a> {
    Leg {
        host,
        profile,
        dirs,
        path: search_path(host),
        target_root: target_root
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from)),
    }
}

/// The search path the gates get: `PATH`, with `ISOLOOM_NODE_BIN` first, the Linux host's
/// `~/.local/bin` (where `host-setup` puts the tools), and on Windows Git's `sh`, which the tests
/// run for the scripts Isoloom renders for Linux machines.
fn search_path(host: Host) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if host == Host::Linux
        && let Some(local) = home.map(|h| h.join(".local/bin")).filter(|d| d.is_dir())
    {
        dirs.insert(0, local);
    }
    if cfg!(windows)
        && let Some(git) = std::env::var_os("ProgramFiles").map(|p| PathBuf::from(p).join("Git").join("bin"))
        && git.join("sh.exe").is_file()
        && !dirs.iter().any(|d| d.join("sh.exe").is_file())
    {
        dirs.push(git);
    }
    if let Some(node) = std::env::var_os("ISOLOOM_NODE_BIN") {
        dirs.insert(0, PathBuf::from(node));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

impl Leg<'_> {
    /// Runs the gates of `repos`, each reported to `sink`.
    pub(super) fn run(&self, repos: &[Repo], sink: &mut dyn Sink) {
        let tool = |name: &str, arg: &str| {
            process::first_line(Command::new(process::program(name, &self.path)).arg(arg).env("PATH", &self.path)).unwrap_or_else(|| "none".into())
        };
        sink.line(&format!(
            "{}: {} {}; {}; node {}",
            self.host.id(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            tool("cargo", "--version"),
            tool("node", "--version")
        ));
        for &repo in repos {
            let dir = self.dirs.of(repo);
            if !dir.is_dir() {
                not_run(sink, repo, "-", Status::Skip, &format!("not found at {}", dir.display()));
                continue;
            }
            match repo {
                Repo::Isoloom => self.gates(sink, repo, &isoloom(self.profile)),
                Repo::Isothrum => self.gates(sink, repo, &isothrum(self.profile)),
                Repo::Launcher => self.launcher(sink),
            }
        }
    }

    fn gates(&self, sink: &mut dyn Sink, repo: Repo, gates: &[Gate]) {
        for g in gates {
            self.gate(sink, repo, g);
        }
    }

    /// Runs one gate in the repository's folder and reports it; whether it passed.
    fn gate(&self, sink: &mut dyn Sink, repo: Repo, g: &Gate) -> bool {
        sink.begin(repo.id(), g.name);
        sink.line(&format!("$ {}", g.argv.join(" ")));
        let Some((program, args)) = g.argv.split_first() else { return false };
        let mut cmd = Command::new(process::program(program, &self.path));
        process::without_run_env(&mut cmd);
        cmd.args(args)
            .current_dir(self.dirs.of(repo))
            .env("PATH", &self.path)
            .env_remove("RUSTUP_TOOLCHAIN");
        cmd.envs(g.env.iter().copied());
        if std::env::var_os("CARGO_BUILD_JOBS").is_none() {
            cmd.env("CARGO_BUILD_JOBS", "4");
        }
        match &self.target_root {
            Some(root) => cmd.env("CARGO_TARGET_DIR", root.join(repo.id())),
            None => cmd.env_remove("CARGO_TARGET_DIR"),
        };
        let start = Instant::now();
        let (status, note) = match process::stream(&mut cmd, &mut |l| sink.line(l)) {
            Ok(s) if s.success() => (Status::Pass, String::new()),
            Ok(s) => (Status::Fail, s.code().map_or_else(|| s.to_string(), |c| format!("exit {c}"))),
            Err(e) => (Status::Fail, format!("can't start {program}: {e}")),
        };
        sink.result(GateResult {
            repo: repo.id().into(),
            gate: g.name.into(),
            status,
            secs: start.elapsed().as_secs(),
            note,
        });
        status == Status::Pass
    }

    /// The launcher's own gate, `pnpm check` (format, types, lint, the web tests, then the Rust
    /// format, lint and tests), after `pnpm install`. Every host runs it.
    fn launcher(&self, sink: &mut dyn Sink) {
        let found = process::first_line(Command::new(process::program("node", &self.path)).arg("--version").env("PATH", &self.path));
        let version = found.unwrap_or_else(|| "none".into());
        if !version.starts_with("v24.") {
            return not_run(
                sink,
                Repo::Launcher,
                "check",
                Status::Fail,
                &format!("Node 24 needed, found {version} (ISOLOOM_NODE_BIN)"),
            );
        }
        if matches!(self.host, Host::Linux | Host::LinuxDocker)
            && process::first_line(Command::new("pkg-config").args(["--exists", "webkit2gtk-4.1"])).is_none()
        {
            let note = format!("no Tauri system packages; as root: apt install {TAURI_PACKAGES}");
            return not_run(sink, Repo::Launcher, "check", Status::Skip, &note);
        }
        // The app embeds ../out at compile time; the tests don't need the real one (as in its CI).
        let out = self.dirs.launcher.join("out");
        if !out.join("index.html").exists()
            && let Err(e) = std::fs::create_dir_all(&out).and_then(|()| std::fs::write(out.join("index.html"), "<!doctype html>\n"))
        {
            sink.line(&format!("can't write {}: {e}", out.join("index.html").display()));
        }
        if self.gate(sink, Repo::Launcher, &gate("install", &["pnpm", "install", "--frozen-lockfile"])) {
            self.gate(sink, Repo::Launcher, &gate("check", &["pnpm", "check"]));
        } else {
            not_run(sink, Repo::Launcher, "check", Status::Skip, "install failed");
        }
    }
}

/// The system packages the launcher's Tauri build needs on Linux.
pub(crate) const TAURI_PACKAGES: &str = "libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf";

/// Records a gate that did not run, with the reason.
fn not_run(sink: &mut dyn Sink, repo: Repo, gate: &str, status: Status, note: &str) {
    sink.begin(repo.id(), gate);
    sink.result(GateResult {
        repo: repo.id().into(),
        gate: gate.into(),
        status,
        secs: 0,
        note: note.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_profile_adds_the_static_gates_before_the_tests() {
        let full: Vec<&str> = isoloom(Profile::Full).iter().map(|g| g.name).collect();
        assert_eq!(
            full,
            ["fmt", "typos", "machete", "clippy", "clippy-linux", "clippy-windows", "doc", "quality", "test"]
        );
        let platform: Vec<&str> = isoloom(Profile::Platform).iter().map(|g| g.name).collect();
        assert_eq!(platform, ["test"]);
        assert_eq!(isothrum(Profile::Full).len(), 3);
    }

    #[test]
    fn the_windows_clippy_leg_names_its_target() {
        let gates = isoloom(Profile::Full);
        let windows = gates.iter().find(|g| g.name == "clippy-windows").map(|g| g.argv.join(" "));
        assert_eq!(
            windows.as_deref(),
            Some("cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc")
        );
    }
}
