//! `isoloom test cloud-services`: the spec's checks, its `{{ cloud.outputs.<name> }}` filled in
//! from the deployed module, run from this machine (scripts get each output as
//! `ISOLOOM_OUTPUT_<NAME>`). No derived checks: cloud services have no machines.

use std::path::Path;
use std::process::{Command, ExitCode, Output};

use indexmap::IndexMap;
use isoloom_core as core;
use isoloom_core::checks::{self, Host, Line, Position};

use crate::tools::terraform;
use crate::{Res, cloud_services, output};

pub fn test(spec: &core::Spec, dir: &Path, instance: Option<u8>, json: bool) -> Res<ExitCode> {
    let outputs = cloud_services::outputs(dir, instance);
    if outputs.is_empty() && spec.cloud.as_ref().is_some_and(|c| !c.outputs.is_empty()) {
        return Err("no outputs: the services aren't deployed (`isoloom run cloud-services`)".into());
    }
    let filled = spec.with_cloud_outputs(&outputs);
    let plan: Vec<_> = checks::plan(&filled).into_iter().filter(|c| !c.derived).collect();
    if plan.is_empty() {
        println!("nothing to check: no `checks:` in the spec");
        return Ok(ExitCode::SUCCESS);
    }
    let out = run_checks(&plan, dir, &outputs)?;
    report(&out, json)
}

/// The checks as one script, run here with the outputs in its environment.
fn run_checks(plan: &[checks::Resolved], dir: &Path, outputs: &IndexMap<String, String>) -> Res<Output> {
    let group: Vec<&checks::Resolved> = plan.iter().collect();
    let host = |h: &Host, _: &Position| match h {
        Host::Literal(l) => l.clone(),
        Host::Machine { name, .. } => name.clone(),
    };
    let run_script = |path: &str| format!("sh {path}");
    let render = checks::Render {
        host: &host,
        script: &run_script,
        playbook: None,
    };
    let script = checks::script(&Position::Networks, &group, &render);
    let mut c = Command::new("sh");
    c.envs(terraform::aws_session_env());
    c.args(["-c", &script]).current_dir(dir);
    for (k, v) in outputs {
        c.env(format!("ISOLOOM_OUTPUT_{}", k.to_uppercase().replace('-', "_")), v);
    }
    Ok(c.output()?)
}

/// The results, as lines or JSON; fails when one failed, or nothing passed and the script did.
fn report(out: &Output, json: bool) -> Res<ExitCode> {
    let text = String::from_utf8_lossy(&out.stdout);
    let (mut passed, mut failed) = (0, 0);
    let mut results = Vec::new();
    for line in text.lines() {
        match checks::parse_line(line) {
            Some(Line::Pass(n)) => {
                passed += 1;
                results.push(serde_json::json!({ "name": n, "ok": true }));
                if !json {
                    println!("  ✓ {n}");
                }
            }
            Some(Line::Fail(n, why)) => {
                failed += 1;
                results.push(serde_json::json!({ "name": n, "ok": false, "detail": why }));
                if !json {
                    println!("  ✗ {n}: {why}");
                }
            }
            _ => {}
        }
    }
    if json {
        output::print_json(&results)?;
    } else {
        println!("\n{passed} passed{}", if failed > 0 { format!(", {failed} failed") } else { String::new() });
    }
    Ok(if failed > 0 || !out.status.success() && passed == 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
