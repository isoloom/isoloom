//! The check runners of the targets run from this host's tools: Docker Compose, Vagrant and
//! kubectl.

use std::path::PathBuf;

use isoloom_core::checks::Position;
use isoloom_core::{Target, shell};

use super::Ctx;
use super::runner::{Runner, STARTING_JOBS};
use crate::tools::{compose, kubectl, process};
use crate::{Res, target};

fn s(x: &str) -> String {
    x.to_string()
}

/// Docker: one `compose run` per position, plus the machines' own `exec` checks.
pub fn compose(ctx: &Ctx) -> Res<Vec<Runner>> {
    let files = compose::compose_files(ctx.dir, ctx.instance);
    start_stand_ins(ctx)?;
    let mut runners = Vec::new();
    for pos in &ctx.positions {
        let mut args = vec![s("compose"), s("--progress"), s("quiet")];
        args.extend(files.iter().cloned());
        args.extend([s("--profile"), s("check"), s("run"), s("--rm"), s("--no-deps")]);
        if ctx.no_derived {
            args.extend([s("-e"), s("ISOLOOM_DERIVED=0")]);
        }
        args.push(ctx.runner_name(pos));
        runners.push(Runner::new(pos.label(), "docker", args, ctx.dir));
        // `exec` checks: their own runner, piped into the machine itself.
        if let Position::Machine(m) = pos
            && let Some(file) = ctx.exec_runner("docker", m)
        {
            let q = shell::quote;
            let compose: Vec<String> = files.iter().map(|f| q(f)).collect();
            let script = format!(
                "docker compose {} exec -T {} sh -s < {}",
                compose.join(" "),
                q(m),
                q(&file.display().to_string())
            );
            runners.push(Runner::shell(format!("inside {m}"), script, ctx.dir));
        }
    }
    Ok(runners)
}

/// The runners run with --no-deps: the environment is up, and `compose run` would otherwise
/// run its completed init jobs again (they are dependencies of the machines the runners wait
/// for), re-seeding it on every test. A stand-in for a supplied access machine isn't up yet:
/// start it (and its routes) first.
fn start_stand_ins(ctx: &Ctx) -> Res<()> {
    let yaml = std::fs::read_to_string(ctx.out.join("docker/compose.yml")).unwrap_or_default();
    let stand_ins: Vec<String> = ["isoloom-access", "isoloom-access-routes"]
        .iter()
        .filter(|n| yaml.contains(&format!("\n  {n}:\n")))
        .map(|n| n.to_string())
        .collect();
    if stand_ins.is_empty() {
        return Ok(());
    }
    let status = compose::quiet(ctx.dir, ctx.instance)
        .args(["--profile", "check", "up", "-d", "--wait", "--no-deps"])
        .args(&stand_ins)
        .status()?;
    if !status.success() {
        return Err("couldn't start the access machine's stand-in".into());
    }
    Ok(())
}

/// Local VMs: Vagrant runs the checks provisioner on every machine.
pub fn vm(ctx: &Ctx, t: Target) -> Runner {
    let args = vec![s("provision"), s("--provision-with"), s("checks")];
    let mut r = Runner::new("every machine", "vagrant", args, &ctx.out.join(target::vm_folder(t)));
    if ctx.no_derived {
        r.env.push(("ISOLOOM_DERIVED".into(), "0".into()));
    }
    r
}

/// Kubernetes: the check Jobs applied fresh, then each one's logs once it has finished, plus
/// the machines' own `exec` checks. Returns the folder of a manifest written for this run.
pub fn kubernetes(ctx: &Ctx) -> Res<(Vec<Runner>, Option<PathBuf>)> {
    let ns = kubectl::namespace(&ctx.spec.name);
    let names: Vec<String> = ctx.positions.iter().map(|p| ctx.runner_name(p)).collect();
    let (manifests, temp) = check_manifests(ctx)?;
    // Fresh Jobs (a Job can't be re-run), then each one's logs once it has finished either way.
    let apply = format!("kubectl -n {ns} delete job {} --ignore-not-found >/dev/null && {manifests}", names.join(" "));
    let mut runners = vec![Runner::shell(STARTING_JOBS, apply, ctx.dir)];
    for pos in &ctx.positions {
        let job = ctx.runner_name(pos);
        let script = format!(
            "i=0; until [ \"$(kubectl -n {ns} get job {job} -o jsonpath='{{.status.conditions[*].type}}' 2>/dev/null)\" != \"\" ]; do i=$((i+2)); [ $i -ge 300 ] && break; sleep 2; done; kubectl -n {ns} logs job/{job} --all-containers 2>/dev/null || true"
        );
        runners.push(Runner::shell(pos.label(), script, ctx.dir));
        // `exec` checks: their own runner, piped into the machine's container.
        if let Position::Machine(m) = pos
            && let Some(file) = ctx.exec_runner("kubernetes", m)
        {
            let script = format!(
                "kubectl -n {ns} exec -i deploy/{m} -c {m} -- sh -s < {}",
                shell::quote(&file.display().to_string())
            );
            runners.push(Runner::shell(format!("inside {m}"), script, ctx.dir));
        }
    }
    Ok((runners, temp))
}

/// The command that applies the check Jobs. They are static manifests: with --no-derived they
/// are rendered here, every runner gets ISOLOOM_DERIVED=0, and that is what gets applied.
fn check_manifests(ctx: &Ctx) -> Res<(String, Option<PathBuf>)> {
    let kdir = ctx.out.join("kubernetes/checks").display().to_string();
    if !ctx.no_derived {
        return Ok((
            format!("kubectl kustomize --load-restrictor LoadRestrictionsNone {kdir} | kubectl apply -f - >/dev/null"),
            None,
        ));
    }
    let patched = kubectl::derived_off(&kubectl::kustomize(&kdir)?)?;
    let dir = process::private_dir("isoloom-k8s")?;
    let file = dir.join("checks.yaml");
    std::fs::write(&file, patched)?;
    Ok((format!("kubectl apply -f {} >/dev/null", shell::quote(&file.display().to_string())), Some(dir)))
}
