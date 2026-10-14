//! `isoloom test cloud-services`: the spec's checks, its `{{ cloud.outputs.<name> }}` filled in
//! from the deployed module, run from this machine (scripts get each output as
//! `ISOLOOM_OUTPUT_<NAME>`). No derived checks: cloud services have no machines.

use std::path::Path;
use std::process::{ExitCode, Output};

use indexmap::IndexMap;
use isoloom_core as core;
use isoloom_core::checks::{self, Host, Line, Position};

use isoloom_core::Target;
use isoloom_core::events::{ErrorCode, Event, Scope};

use crate::failure::fail;
use crate::tools::terraform;
use crate::{Res, cloud_services, output};

pub fn test(spec: &core::Spec, dir: &Path, instance: Option<u8>) -> Res<ExitCode> {
    let outputs = cloud_services::outputs(dir, instance);
    if outputs.is_empty() && spec.cloud.as_ref().is_some_and(|c| !c.outputs.is_empty()) {
        return Err(fail(
            ErrorCode::NotRunning,
            "no outputs: the services aren't deployed (`isoloom run cloud-services`)",
        ));
    }
    let filled = spec.with_cloud_outputs(&outputs);
    let plan: Vec<_> = checks::plan(&filled).into_iter().filter(|c| !c.derived).collect();
    if plan.is_empty() {
        if output::json() {
            output::result("test", true, summary(0, 0, &[]));
        } else {
            println!("nothing to check: no `checks:` in the spec");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let at = Scope::step(Some(Target::CloudServices.id()), "checks");
    output::begin(&at, None);
    let out = run_checks(&plan, dir, &outputs)?;
    output::done(&at, None);
    Ok(report(&out, &at))
}

/// The `result` event's data.
fn summary(passed: usize, failed: usize, results: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({ "target": Target::CloudServices.id(), "passed": passed, "failed": failed, "unfinished": [], "results": results })
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
    let mut c = crate::tools::process::command("sh");
    c.envs(terraform::aws_session_env());
    c.args(["-c", &script]).current_dir(dir);
    for (k, v) in outputs {
        c.env(format!("ISOLOOM_OUTPUT_{}", k.to_uppercase().replace('-', "_")), v);
    }
    Ok(c.output()?)
}

/// The results, as lines or events; fails when one failed, or nothing passed and the script did.
fn report(out: &Output, at: &Scope) -> ExitCode {
    let json = output::json();
    let text = String::from_utf8_lossy(&out.stdout);
    let (mut passed, mut failed) = (0, 0);
    let mut results = Vec::new();
    for line in text.lines() {
        let (name, ok, detail) = match checks::parse_line(line) {
            Some(Line::Pass(n)) => (n, true, String::new()),
            Some(Line::Fail(n, why)) => (n, false, why),
            _ => continue,
        };
        if ok {
            passed += 1;
        } else {
            failed += 1;
        }
        results.push(serde_json::json!({ "position": "networks", "name": name, "ok": ok, "detail": detail }));
        if json {
            output::events().emit(&Event::Check {
                target: at.target.clone(),
                position: "networks".into(),
                name,
                ok,
                detail,
                time: core::registry::now(),
            });
        } else if ok {
            println!("  ✓ {name}");
        } else {
            println!("  ✗ {name}: {detail}");
        }
    }
    let ok = !(failed > 0 || !out.status.success() && passed == 0);
    if json {
        output::result("test", ok, summary(passed, failed, &results));
    } else {
        println!("\n{passed} passed{}", if failed > 0 { format!(", {failed} failed") } else { String::new() });
    }
    output::exit_code(ok)
}
