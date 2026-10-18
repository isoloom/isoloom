//! What the container said, read back: per module, its `terraform test -json` stream turned into
//! each run's status and diagnostics.
//!
//! The container's script (`run.sh`) writes, for each module in turn, a `@@module <folder>`
//! line, a `@@status <exit status or init-failed>` line, Terraform's JSON lines, and anything else
//! as `@@log <line>`.

use serde_json::Value;

/// One module's results.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ModuleResult {
    /// Its folder in the tree (`segmented/.isoloom/cloud-vm/aws`).
    pub(crate) module: String,
    /// `terraform test`'s exit status, or `init-failed`.
    pub(crate) status: String,
    /// Its runs, in the order they finished.
    pub(crate) runs: Vec<RunResult>,
    /// Every run its test files hold, as Terraform listed them before starting (file, name).
    pub(crate) planned: Vec<(String, String)>,
    /// The errors no run owns: the test file's or the configuration's.
    pub(crate) errors: Vec<Diagnostic>,
    /// What else it printed (`terraform init`'s output when that failed).
    pub(crate) log: Vec<String>,
}

/// One `run` block's result.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RunResult {
    /// Its test file, as Terraform names it (`tests/segmented.tftest.hcl`).
    pub(crate) file: String,
    /// The run's name.
    pub(crate) name: String,
    /// `pass`, `fail` (an assertion failed), `error` or `skip`.
    pub(crate) status: String,
    /// Its failed assertions and errors.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// A failed assertion or an error.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// Terraform's summary (`Test assertion failed`).
    pub(crate) summary: String,
    /// Its detail: an assertion's `error_message`.
    pub(crate) detail: String,
    /// The line it points at, in its file.
    pub(crate) line: Option<u64>,
    /// The planned values the expression read, each as Terraform states it
    /// (`aws_subnet.back.cidr_block`, `is "10.61.20.0/24"`).
    pub(crate) values: Vec<(String, String)>,
    /// An `==` or `!=` assertion's two sides, as planned and as written.
    pub(crate) sides: Option<(String, String)>,
    /// What Terraform noted on the comparison (`LHS and RHS values are of different types`).
    pub(crate) note: String,
}

/// Every module's results, from the container's output.
#[must_use]
pub(crate) fn parse(output: &str) -> Vec<ModuleResult> {
    let mut modules: Vec<ModuleResult> = Vec::new();
    for line in output.lines() {
        if let Some(dir) = line.strip_prefix("@@module ") {
            modules.push(ModuleResult {
                module: dir.trim().to_owned(),
                ..ModuleResult::default()
            });
        } else if let Some(current) = modules.last_mut() {
            read_line(current, line);
        }
    }
    modules
}

fn read_line(m: &mut ModuleResult, line: &str) {
    if let Some(status) = line.strip_prefix("@@status ") {
        status.trim().clone_into(&mut m.status);
    } else if let Some(text) = line.strip_prefix("@@log ") {
        m.log.push(text.to_owned());
    } else if let Ok(json) = serde_json::from_str::<Value>(line) {
        read_json(m, &json);
    } else if !line.trim().is_empty() {
        m.log.push(line.to_owned());
    }
}

fn text<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or_default()
}

fn read_json(m: &mut ModuleResult, json: &Value) {
    let (file, run) = (text(json, "/@testfile"), text(json, "/@testrun"));
    match text(json, "/type") {
        "test_abstract" => {
            let files = json.get("test_abstract").and_then(Value::as_object).into_iter().flatten();
            for (f, runs) in files {
                let names = runs.as_array().into_iter().flatten().filter_map(Value::as_str);
                m.planned.extend(names.map(|n| (f.clone(), n.to_owned())));
            }
        }
        "test_run" if text(json, "/test_run/progress") == "complete" => {
            let status = text(json, "/test_run/status");
            with_run(m, file, run, |r| status.clone_into(&mut r.status));
        }
        "diagnostic" if text(json, "/diagnostic/severity") == "error" => {
            let d = diagnostic(json);
            if run.is_empty() {
                m.errors.push(d);
            } else {
                with_run(m, file, run, |r| r.diagnostics.push(d));
            }
        }
        _ => {}
    }
}

/// Applies `f` to the run named `name` in `file`, added first when it isn't there yet (a
/// diagnostic can come before its run's status).
fn with_run(m: &mut ModuleResult, file: &str, name: &str, f: impl FnOnce(&mut RunResult)) {
    if let Some(r) = m.runs.iter_mut().find(|r| r.file == file && r.name == name) {
        f(r);
    } else {
        let mut r = RunResult {
            file: file.to_owned(),
            name: name.to_owned(),
            ..RunResult::default()
        };
        f(&mut r);
        m.runs.push(r);
    }
}

fn diagnostic(json: &Value) -> Diagnostic {
    let values = json
        .pointer("/diagnostic/snippet/values")
        .and_then(Value::as_array)
        .map(|vs| {
            vs.iter()
                .map(|v| (text(v, "/traversal").to_owned(), text(v, "/statement").to_owned()))
                .collect()
        })
        .unwrap_or_default();
    Diagnostic {
        summary: text(json, "/diagnostic/summary").to_owned(),
        detail: text(json, "/diagnostic/detail").to_owned(),
        line: json.pointer("/diagnostic/range/start/line").and_then(Value::as_u64),
        values,
        sides: json
            .pointer("/diagnostic/snippet/test_assertion_expr")
            .map(|e| (text(e, "/lhs").to_owned(), text(e, "/rhs").to_owned())),
        note: text(json, "/diagnostic/snippet/test_assertion_expr/warning").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = r#"@@module segmented/.isoloom/cloud-vm/aws
@@status 1
{"@level":"info","type":"version","terraform":"1.16.6"}
{"type":"test_abstract","test_abstract":{"tests/segmented.tftest.hcl":["routing","firewall","outputs"]}}
{"@testfile":"tests/segmented.tftest.hcl","@testrun":"routing","test_run":{"path":"tests/segmented.tftest.hcl","run":"routing","progress":"complete","status":"fail"},"type":"test_run"}
{"@testfile":"tests/segmented.tftest.hcl","@testrun":"routing","diagnostic":{"severity":"error","summary":"Test assertion failed","detail":"back has no internet route","range":{"filename":"tests/segmented.tftest.hcl","start":{"line":16}},"snippet":{"values":[{"traversal":"aws_subnet.back.cidr_block","statement":"is \"10.61.20.0/24\""}],"test_assertion_expr":{"lhs":"\"10.61.20.0/24\"","rhs":"\"10.0.0.0/24\""}}},"type":"diagnostic"}
{"@testfile":"tests/segmented.tftest.hcl","@testrun":"firewall","test_run":{"progress":"complete","status":"pass"},"type":"test_run"}
@@module solo-web/.isoloom/proxmox
@@status init-failed
@@log Error: Failed to query available provider packages
"#;

    #[test]
    fn each_module_gets_its_runs_and_their_failed_assertions() {
        let ms = parse(OUTPUT);
        assert_eq!(ms.len(), 2);
        let aws = &ms[0];
        assert_eq!((aws.module.as_str(), aws.status.as_str()), ("segmented/.isoloom/cloud-vm/aws", "1"));
        let names: Vec<(&str, &str)> = aws.runs.iter().map(|r| (r.name.as_str(), r.status.as_str())).collect();
        assert_eq!(names, [("routing", "fail"), ("firewall", "pass")]);
        assert_eq!(aws.planned.len(), 3, "outputs never finished: {:?}", aws.planned);
        let d = &aws.runs[0].diagnostics[0];
        assert_eq!((d.detail.as_str(), d.line), ("back has no internet route", Some(16)));
        assert_eq!(d.values, [("aws_subnet.back.cidr_block".to_owned(), "is \"10.61.20.0/24\"".to_owned())]);
        assert_eq!(d.sides, Some(("\"10.61.20.0/24\"".to_owned(), "\"10.0.0.0/24\"".to_owned())));
    }

    #[test]
    fn a_module_whose_init_failed_keeps_what_init_said() {
        let ms = parse(OUTPUT);
        assert_eq!(ms[1].status, "init-failed");
        assert_eq!(ms[1].log, ["Error: Failed to query available provider packages"]);
        assert_eq!(ms[1].runs.len(), 0, "no run started");
    }

    #[test]
    fn an_error_no_run_owns_is_the_module_s() {
        let out = "@@module m\n{\"type\":\"diagnostic\",\"diagnostic\":{\"severity\":\"error\",\"summary\":\"unknown provider\"}}\n{\"type\":\"diagnostic\",\"diagnostic\":{\"severity\":\"warning\",\"summary\":\"deprecated\"}}\n";
        let ms = parse(out);
        assert_eq!(ms[0].errors.len(), 1, "warnings are left out");
        assert_eq!(ms[0].errors[0].summary, "unknown provider");
    }
}
