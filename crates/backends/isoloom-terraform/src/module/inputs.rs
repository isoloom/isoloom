//! The values a module is given at launch, each a variable of its own, named as Compose and a
//! Vagrantfile read it from their environment, so one set of values starts every format (with
//! Terraform, as `TF_VAR_<name>`):
//!
//! - an input (`inputs:`) under its own name, sensitive unless the spec says it isn't, empty when
//!   not given;
//! - a secret a machine gets as a file (`secrets:`) as `ISOLOOM_SECRET_<NAME>`, sensitive, with no
//!   default: Terraform refuses to apply without it.
//!
//! A machine reads them from its set-up: the inputs file (`KEY='value'` lines) and its secret
//! files, written from the variables. Each value lands in the user data (or the cloud-init
//! snippet) and in Terraform's state, as every value a module takes does.

use crate::hcl_builder::{Block, Expression, Template, call, list, reference, string};
use isoloom_plan::{Environment, MachinePlan, MachineShape, SecretFile, ValueSource};
use isoloom_render::steps;
use isoloom_spec::contract::secret_variable;

use super::variable::Variable;

/// The variables of the values the VMs of `env` read: each input one reads (in the spec's
/// order), then each secret one gets as a file.
pub(crate) fn variables_of(env: &Environment) -> Vec<Variable> {
    let vms = || env.machines.values().filter(|mp| mp.shape == MachineShape::Vm);
    let mut out: Vec<Variable> = env
        .inputs
        .iter()
        .filter(|i| vms().any(|mp| mp.reads_input(&i.name)))
        .map(|i| {
            let v = Variable::new(&i.name, "string")
                .default(string(""))
                .describe("A value given at launch (the spec's `inputs`): empty when not given");
            if i.sensitive { v.sensitive() } else { v }
        })
        .collect();
    let mut secrets: Vec<&str> = Vec::new();
    for mp in vms() {
        for f in &mp.secrets {
            if let ValueSource::Secret(name) = &f.source
                && !secrets.contains(&name.as_str())
            {
                secrets.push(name);
            }
        }
    }
    let order = |name: &str| env.spec().secrets.keys().position(|k| k == name).unwrap_or(usize::MAX);
    secrets.sort_by_key(|n| order(n));
    out.extend(secrets.into_iter().map(|name| {
        Variable::new(&secret_variable(name), "string")
            .sensitive()
            .describe(&format!("The secret `{name}` (the spec's `secrets`), which the machines get as a file"))
    }));
    out
}

/// [`variables_of`] as blocks.
pub(crate) fn variables(env: &Environment) -> Vec<Block> {
    variables_of(env).into_iter().map(Variable::block).collect()
}

/// `var.<input>`: input `name`'s value.
fn input(name: &str) -> Expression {
    reference(&format!("var.{name}"))
}

/// A machine's inputs file (`KEY='value'` lines, read with `sh`'s `.`), from each input's
/// variable. Each value is single-quoted (its quotes written `'\''`), so `sh` takes it as it is:
/// no `$`, backtick or `$(...)` expands.
pub(crate) fn file(inputs: &[String]) -> Expression {
    let lines = inputs.iter().map(|i| {
        Template::text(format!("{i}='"))
            .with_expr(call("replace", [input(i), string("'"), string("'\\''")]))
            .with_text("'")
            .into_expression()
    });
    call("join", [string("\n"), list(lines)])
}

/// The inputs file of `names` written owner-only at [`steps::KEPT_INPUTS_FILE`] by a machine's
/// own set-up, from their variables, base64 on its way so no value can end the script it travels
/// in (its `umask` in a subshell: the ready file the set-up writes next stays readable).
pub(crate) fn written(names: &[String]) -> Template {
    Template::text(format!("{} && (umask 077 && echo ", steps::MAKE_STATE_DIR))
        .with_expr(call("base64encode", [file(names)]))
        .with_text(format!(" | base64 -d > {})", steps::KEPT_INPUTS_FILE))
}

/// The variable file `f`'s value comes from: its input's, or its secret's.
fn value_of(f: &SecretFile) -> Expression {
    match &f.source {
        ValueSource::Input(name) => input(name),
        ValueSource::Secret(name) => reference(&format!("var.{}", secret_variable(name))),
    }
}

/// A machine's `Secrets` step on a machine Terraform gives the values to (a cloud machine's user
/// data): each value, from its variable in base64, in the environment of the step that keeps it
/// and puts the files in place, now and at every boot (`isoloom_render::secrets::from_environment`).
/// Nothing waits: the values are there. `None` when the machine gets no secret.
pub(crate) fn secrets_step(mp: &MachinePlan) -> Option<Template> {
    let (unique, values) = kept_values(mp)?;
    let script = isoloom_render::secrets::from_environment(&values, &mp.secrets);
    let mut t = Template::text("env");
    for (f, (_, var)) in unique.iter().zip(&values) {
        t.push_text(format!(" {var}="));
        t.push_expr(call("base64encode", [value_of(f)]));
    }
    t.push_text(format!(" sh -c {}", isoloom_spec::shell::quote(script.trim_end_matches('\n'))));
    Some(t)
}

/// A server machine's secrets as a file of `NAME='<base64>'` lines, one per value its set-up
/// keeps, each from its variable: cloud-init writes it (Proxmox) or Terraform uploads it over SSH
/// (ESXi), and [`secrets_from_file`] reads it. `None` when the machine gets no secret.
pub(crate) fn secrets_file(mp: &MachinePlan) -> Option<Expression> {
    let (unique, values) = kept_values(mp)?;
    let lines = unique.iter().zip(&values).map(|(f, (_, var))| {
        Template::text(format!("{var}='"))
            .with_expr(call("base64encode", [value_of(f)]))
            .with_text("'")
            .into_expression()
    });
    Some(call("join", [string(""), list(lines.map(|l| call("format", [string("%s\n"), l])))]))
}

/// The `Secrets` step reading [`secrets_file`] at `path` (run as root): the values exported from
/// it, the file removed, then each kept and the files put in place now and at every boot
/// (`isoloom_render::secrets::from_environment`). `None` when the machine gets no secret.
pub(crate) fn secrets_from_file(mp: &MachinePlan, path: &str) -> Option<String> {
    let (_, values) = kept_values(mp)?;
    let script = isoloom_render::secrets::from_environment(&values, &mp.secrets);
    Some(format!(
        "set -a && . {path} && set +a && rm -f {path} && sh -c {}",
        isoloom_spec::shell::quote(script.trim_end_matches('\n'))
    ))
}

/// A machine's secret files, each once, and each one's kept name with the variable its value
/// travels in.
type Kept<'a> = (Vec<&'a SecretFile>, Vec<(String, String)>);

/// Each secret file of `mp` once by the name its value is kept under, with that name and the
/// variable its value travels in. `None` when it has none.
fn kept_values(mp: &MachinePlan) -> Option<Kept<'_>> {
    if mp.secrets.is_empty() {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    let unique: Vec<&SecretFile> = mp.secrets.iter().filter(|f| seen.insert(f.kept().to_owned())).collect();
    let values = unique.iter().map(|f| (f.kept().to_owned(), secret_variable(&f.name))).collect();
    Some((unique, values))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;
    use crate::hcl_builder::Piece;

    const SPEC: &str = "version: 1\nname: s\ninputs:\n  TOKEN: {}\n  MODE: { sensitive: false }\n  UNREAD: {}\nsecrets:\n  db-password: { env: DB_PASSWORD }\nnetworks:\n  lan: { cidr: 10.61.10.0/24 }\nmachines:\n  web:\n    os: debian@12\n    networks: { lan: 10 }\n    environment: { MODE: { input: MODE } }\n    files:\n      - { path: /etc/token, input: TOKEN }\n      - { path: /etc/db, secret: db-password }\n";

    #[test]
    fn each_value_a_vm_reads_is_a_variable_of_its_own_name() {
        let env = crate::testing::planned(SPEC, Target::CloudVm);
        let vars = variables_of(&env);
        let names: Vec<&str> = vars.iter().map(Variable::name).collect();
        assert_eq!(names, ["TOKEN", "MODE", "ISOLOOM_SECRET_DB_PASSWORD"]);
        assert!(vars[0].is_sensitive() && !vars[1].is_sensitive() && vars[2].is_sensitive());
        assert!(
            vars[0].has_default() && !vars[2].has_default(),
            "a secret has no default: Terraform asks for it"
        );
    }

    #[test]
    fn the_secrets_step_hands_each_value_over_in_base64_and_waits_for_nothing() {
        let env = crate::testing::planned(SPEC, Target::CloudVm);
        let step = secrets_step(&env.machines["web"]).unwrap();
        let text: String = step
            .pieces()
            .map(|p| match p {
                Piece::Text(s) => s.to_owned(),
                Piece::Expr(e) => format!("<{e:?}>"),
            })
            .collect();
        assert!(text.starts_with("env ISOLOOM_SECRET_INPUT_TOKEN=<"), "{text}");
        assert!(
            text.contains("Identifier(TOKEN)") && text.contains("Identifier(ISOLOOM_SECRET_DB_PASSWORD)"),
            "{text}"
        );
        assert!(!text.contains("waiting"), "nothing waits: {text}");
        assert!(text.contains("touch /var/lib/isoloom/secrets/.delivered"), "{text}");
    }
}
