//! The check runners of the targets whose machines are reached over SSH: the clouds, Proxmox
//! and existing machines.

use std::path::Path;

use isoloom_plan::checks::Position;

use super::Ctx;
use super::runner::{Runner, piped, ssh_runner};
use isoloom_runtime::events::ErrorCode;

use crate::external::external_machines;
use crate::failure::fail;
use crate::tools::terraform;
use crate::{Res, target};

/// The label of a runner the module's `checks` output places: on the networks, or a machine.
fn from_position(position: &str) -> String {
    if position == "networks" {
        "from the environment's networks".to_string()
    } else {
        format!("from {position}")
    }
}

/// The module's `checks` output: one entry per runner, with where it runs.
fn checks_output(outputs: &serde_json::Value, target: &str) -> Res<Vec<serde_json::Value>> {
    match outputs.get("checks").and_then(|v| v.as_array()) {
        Some(list) => Ok(list.clone()),
        None => Err(fail(
            ErrorCode::NotRunning,
            format!("the module has no `checks` output: run `isoloom run {target}` first"),
        )),
    }
}

/// Cloud VMs: each runner over SSH, on the host the module placed it.
pub fn cloud_vm(ctx: &Ctx) -> Res<Vec<Runner>> {
    let cloud = target::vm_cloud(ctx.dir, ctx.instance, ctx.spec, ctx.cloud)?;
    let module = ctx.out.join("cloud-vm").join(cloud);
    let outputs = terraform::output(&module)?;
    let list = checks_output(&outputs, "cloud-vm")?;
    let runners = list
        .iter()
        .map(|entry| {
            let host = entry["host"].as_str().unwrap_or_default();
            let user = entry["user"].as_str().unwrap_or("root");
            let command = entry["command"].as_str().unwrap_or_default();
            let position = entry["position"].as_str().unwrap_or_default();
            let mut r = ssh_runner(ctx.ssh_key, user, host, command, ctx.no_derived);
            r.label = from_position(position);
            r
        })
        .collect();
    Ok(runners)
}

/// Docker on a cloud host: one `compose run` per position, over SSH.
pub fn cloud_docker(ctx: &Ctx) -> Res<Vec<Runner>> {
    let cloud = target::docker_cloud(ctx.cloud)?;
    let outputs = terraform::output(&ctx.out.join("cloud-docker").join(cloud))?;
    let host = outputs["ip"]
        .as_str()
        .ok_or_else(|| fail(ErrorCode::NotRunning, "the module has no `ip` output: run `isoloom run cloud-docker` first"))?;
    let user = outputs["ssh_user"].as_str().unwrap_or("root");
    let runners = ctx
        .positions
        .iter()
        .map(|pos| {
            let command = format!(
                "cd /opt/isoloom && sudo docker compose -f .isoloom/docker/compose.yml --profile check run --rm -e ISOLOOM_DERIVED {}",
                ctx.runner_name(pos)
            );
            let mut r = ssh_runner(ctx.ssh_key, user, host, &command, ctx.no_derived);
            r.label = pos.label();
            r
        })
        .collect();
    Ok(runners)
}

/// Proxmox and ESXi (`dir`, the target's folder): each runner piped to its machine over SSH,
/// through the router.
pub fn behind_router(ctx: &Ctx, dir: &str) -> Res<Vec<Runner>> {
    let outputs = terraform::output(&ctx.out.join(dir))?;
    let router = outputs["address"]
        .as_str()
        .ok_or_else(|| {
            fail(
                ErrorCode::NotRunning,
                format!("the module has no `address` output: run `isoloom run {dir}` first"),
            )
        })?
        .to_string();
    let list = checks_output(&outputs, dir)?;
    let runners = list
        .iter()
        .map(|entry| {
            let host = entry["host"].as_str().unwrap_or_default();
            let user = entry["user"].as_str().unwrap_or("isoloom");
            let position = entry["position"].as_str().unwrap_or_default();
            let script = ctx.out.join(dir).join("checks").join(format!("{position}.sh"));
            let mut r = ssh_runner(ctx.ssh_key, user, host, &format!("{}sh -s", ctx.derived_prefix()), false);
            r.args.insert(0, format!("isoloom@{router}"));
            r.args.insert(0, "-J".to_string());
            let mut r = piped(r, &script);
            r.label = from_position(position);
            r
        })
        .collect();
    Ok(runners)
}

/// Existing machines: each machine's runner, piped to its shell over SSH.
pub fn external(ctx: &Ctx) -> Res<Vec<Runner>> {
    let machines = external_machines(&ctx.out)?;
    let mut runners = Vec::new();
    for pos in &ctx.positions {
        let Position::Machine(m) = pos else { continue };
        let Some(e) = machines.get(m) else { continue };
        let script = ctx.out.join("external/checks").join(format!("{m}.sh"));
        if !script.exists() {
            continue;
        }
        let key = ctx.ssh_key.or(e.key.as_deref().map(Path::new));
        let mut r = ssh_runner(key, &e.user, &e.address, &format!("{}sh -s", ctx.derived_prefix()), false);
        r.args.insert(r.args.len() - 2, format!("-p{}", e.port));
        // `sh -s < script`: through a shell so the runner's stdin is the file.
        let mut r = piped(r, &script);
        r.label = pos.label();
        runners.push(r);
    }
    Ok(runners)
}
