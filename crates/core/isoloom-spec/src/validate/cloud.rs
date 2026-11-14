//! `cloud`: cloud services, a Terraform module and what it exposes, with no machines.

use super::Problems;
use crate::model::{CloudServices, Spec};

/// What `cloud.env` can't set, though plainly named: the shell's and the session's own, and the
/// ones that redirect where a tool finds its certificates or sends its traffic.
const CLOUD_ENV_RESERVED: &[&str] = &[
    "PATH",
    "HOME",
    "IFS",
    "ENV",
    "CDPATH",
    "PS4",
    "SHELL",
    "SHELLOPTS",
    "PROMPT_COMMAND",
    "TMPDIR",
    "TMP",
    "TEMP",
    "USER",
    "LOGNAME",
    "PWD",
    "OLDPWD",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CURL_CA_BUNDLE",
    "REQUESTS_CA_BUNDLE",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

/// The families `cloud.env` leaves alone: what the dynamic loader, an interpreter, Git, SSH or
/// a container or VM tool reads to change what it runs; Terraform's own settings (its variables
/// are `TF_VAR_<name>`); the clouds' credentials, profiles and endpoints, which come from the
/// user's own environment only; and Isoloom's.
const CLOUD_ENV_RESERVED_PREFIXES: &[&str] = &[
    "LD_",
    "DYLD_",
    "BASH",
    "GCONV",
    "GLIBC_",
    "MALLOC_",
    "XDG_",
    "PYTHON",
    "PERL",
    "RUBY",
    "NODE_",
    "JAVA_",
    "GIT_",
    "SSH_",
    "DOCKER_",
    "KUBE",
    "VAGRANT_",
    "TF_",
    "AWS_",
    "ARM_",
    "AZURE_",
    "GOOGLE_",
    "CLOUDSDK_",
    "OCI_",
    "DIGITALOCEAN_",
    "LINODE_",
    "ISOLOOM_",
];

/// The cloud settings `cloud.env` may set inside the reserved families: where the module runs.
const CLOUD_ENV_REGIONS: &[&str] = &[
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
    "GOOGLE_REGION",
    "GOOGLE_ZONE",
    "CLOUDSDK_COMPUTE_REGION",
    "CLOUDSDK_COMPUTE_ZONE",
];

/// Terraform's variable the module's instance name comes in, the scripts' own.
const INSTANCE_VAR: &str = "TF_VAR_isoloom_instance";

/// The most applies `cloud.apply_retries` adds after a failed one.
const MAX_APPLY_RETRIES: u8 = 5;

/// A Terraform variable's or output's name.
fn ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') && !s.starts_with(|ch: char| ch.is_ascii_digit())
}

/// A plain environment variable's name: an upper-case letter, then upper-case letters, digits
/// and `_` (`REGION`, `APP_MODE`); or a Terraform variable's (`TF_VAR_<name>`).
fn env_name(s: &str) -> bool {
    let plain = s.starts_with(|ch: char| ch.is_ascii_uppercase()) && s.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
    plain || s.strip_prefix("TF_VAR_").is_some_and(ident)
}

/// Whether `cloud.env` may set `name` (a well-formed one): a Terraform variable other than the
/// scripts' own, a region setting, or any name outside the reserved ones and families.
fn env_allowed(name: &str) -> bool {
    if name.starts_with("TF_VAR_") {
        return name != INSTANCE_VAR;
    }
    CLOUD_ENV_REGIONS.contains(&name) || !(CLOUD_ENV_RESERVED.contains(&name) || CLOUD_ENV_RESERVED_PREFIXES.iter().any(|p| name.starts_with(p)))
}

/// A tool `cloud.requires` names: a command's bare name (`jq`, `gsed`, `python3`), which a
/// runner looks up and asks for its `--version`, nothing else.
fn tool_name(s: &str) -> bool {
    s.len() <= 64
        && s.starts_with(|ch: char| ch.is_ascii_alphanumeric())
        && s.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '+' | '-'))
}

/// The cloud services: alone in the environment, a module, well-named variables and outputs,
/// inputs that are declared, a sane hourly cost, and the scripts' environment, retries and
/// host requirements.
pub(super) fn check(spec: &Spec, p: &mut Problems) {
    let Some(c) = &spec.cloud else { return };
    if !spec.machines.is_empty() {
        p.add("cloud", "an environment is cloud services or machines, not both: leave `machines` out");
    }
    if c.terraform.trim().is_empty() {
        p.add("cloud.terraform", "the Terraform module's folder, in the project");
    }
    for k in c.vars.keys() {
        if !ident(k) {
            p.add(format!("cloud.vars.{k}"), format!("`{k}` isn't a Terraform variable name"));
        }
    }
    for (k, o) in &c.outputs {
        if !ident(k) || !ident(o) {
            p.add(format!("cloud.outputs.{k}"), format!("`{k}: {o}`: a name, then the module output it reads"));
        }
    }
    for (k, v) in &c.vars {
        for i in CloudServices::inputs_in(v) {
            if !spec.declares_input(&i) {
                p.unknown(format!("cloud.vars.{k}"), format!("`{i}` isn't declared in the spec's `inputs`"));
            }
        }
    }
    if c.hourly_usd.is_some_and(|h| !(0.0..=1000.0).contains(&h)) {
        p.add("cloud.hourly_usd", "a cost per hour in US dollars (0 to 1000)");
    }
    scripts(c, p);
}

/// What the scripts take from the spec: `env`'s names, `apply_retries`, `requires`.
fn scripts(c: &CloudServices, p: &mut Problems) {
    for k in c.env.keys() {
        if !env_allowed(k) {
            p.add(
                format!("cloud.env.{k}"),
                format!("`{k}` changes how the tools on this computer run, or is the scripts' own: credentials and tool settings come from the user's own environment; pick a name of the module's"),
            );
        } else if !env_name(k) {
            p.add(
                format!("cloud.env.{k}"),
                format!("`{k}` isn't a plain environment variable name (upper-case letters, digits, `_`, starting with a letter) or a Terraform variable (`TF_VAR_<name>`)"),
            );
        }
    }
    if c.apply_retries > MAX_APPLY_RETRIES {
        p.add("cloud.apply_retries", format!("at most {MAX_APPLY_RETRIES} more applies"));
    }
    for (i, r) in c.requires.iter().enumerate() {
        if !tool_name(r) {
            p.add(
                format!("cloud.requires[{i}]"),
                format!("`{r}` isn't a command's name: name the tool alone (`jq`), which a runner finds on the PATH and asks for its `--version`"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::problems;

    /// The problems of a cloud-services environment written as `cloud`, its input `OPERATOR_IP`
    /// declared.
    fn of(cloud: &str) -> Vec<String> {
        problems(&format!("version: 1\nname: t\ninputs: [OPERATOR_IP]\ncloud: {cloud}\n"))
    }

    /// The problems of `cloud.<field>` added to a sound module.
    fn with(field: &str) -> Vec<String> {
        of(&format!("{{ provider: aws, terraform: infra, {field} }}"))
    }

    #[test]
    fn sound_cloud_services_pass() {
        let ok = with(
            "vars: { region: eu, ip-list_2: ['{{ inputs.OPERATOR_IP }}/32'] }, outputs: { site: site_url, api-2_x: api }, hourly_usd: 0, env: { REGION: x, AWS_REGION: eu-west-3, TF_VAR_size: s, A_1: z }, apply_retries: 5, requires: [jq, python3, gsed, linode-cli]",
        );
        assert_eq!(ok, Vec::<String>::new());
        assert_eq!(with("hourly_usd: 1000"), Vec::<String>::new());
    }

    #[test]
    fn cloud_services_have_no_machines_and_a_module() {
        let both = problems(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines: { web: { networks: { lan: 10 }, docker: { image: nginx } } }\ncloud: { provider: aws, terraform: infra }\n",
        );
        assert_eq!(both, ["cloud: an environment is cloud services or machines, not both: leave `machines` out"]);
        assert_eq!(
            of("{ provider: aws, terraform: ' ' }"),
            ["cloud.terraform: the Terraform module's folder, in the project"]
        );
    }

    #[test]
    fn variables_and_outputs_are_terraform_names() {
        for bad in ["1region", "re.gion", "re gion"] {
            assert_eq!(
                with(&format!("vars: {{ '{bad}': x }}")),
                [format!("cloud.vars.{bad}: `{bad}` isn't a Terraform variable name")],
                "{bad}"
            );
        }
        assert_eq!(with("vars: { '': x }"), ["cloud.vars.: `` isn't a Terraform variable name"]);
        assert_eq!(
            with("outputs: { 1site: site_url }"),
            ["cloud.outputs.1site: `1site: site_url`: a name, then the module output it reads"]
        );
        assert_eq!(
            with("outputs: { site: site.url }"),
            ["cloud.outputs.site: `site: site.url`: a name, then the module output it reads"]
        );
    }

    #[test]
    fn a_variables_inputs_are_declared() {
        assert_eq!(
            with("vars: { ip: '{{ inputs.NOPE }}' }"),
            ["cloud.vars.ip: `NOPE` isn't declared in the spec's `inputs`"]
        );
    }

    #[test]
    fn the_hourly_cost_is_0_to_1000_dollars() {
        for bad in ["-1", "1000.5"] {
            assert_eq!(
                with(&format!("hourly_usd: {bad}")),
                ["cloud.hourly_usd: a cost per hour in US dollars (0 to 1000)"],
                "{bad}"
            );
        }
    }

    #[test]
    fn the_scripts_environment_is_well_named_and_their_own_is_left_alone() {
        for bad in ["1A", "A-B", "_TOKEN", "region", "TF_VAR_1x"] {
            let why = format!(
                "cloud.env.{bad}: `{bad}` isn't a plain environment variable name (upper-case letters, digits, `_`, starting with a letter) or a Terraform variable (`TF_VAR_<name>`)"
            );
            assert_eq!(with(&format!("env: {{ {bad}: x }}")), [why], "{bad}");
        }
        let reserved = [
            "PATH",
            "HOME",
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
            "TF_CLI_ARGS",
            "TF_CLI_ARGS_apply",
            "TF_CLI_CONFIG_FILE",
            "TF_PLUGIN_CACHE_DIR",
            "TF_DATA_DIR",
            "TF_VAR_isoloom_instance",
            "BASH_ENV",
            "SSL_CERT_FILE",
            "HTTPS_PROXY",
            "PYTHONPATH",
            "NODE_OPTIONS",
            "GIT_SSH_COMMAND",
            "DOCKER_HOST",
            "AWS_ENDPOINT_URL",
            "AWS_PROFILE",
            "ISOLOOM_PROJECT",
        ];
        for taken in reserved {
            let why = format!(
                "cloud.env.{taken}: `{taken}` changes how the tools on this computer run, or is the scripts' own: credentials and tool settings come from the user's own environment; pick a name of the module's"
            );
            assert_eq!(with(&format!("env: {{ {taken}: x }}")), [why], "{taken}");
        }
    }

    #[test]
    fn retries_and_requirements_are_bounded_and_named() {
        assert_eq!(with("apply_retries: 6"), ["cloud.apply_retries: at most 5 more applies"]);
        let not_a_name = |i: usize, r: &str| {
            format!(
                "cloud.requires[{i}]: `{r}` isn't a command's name: name the tool alone (`jq`), which a runner finds on the PATH and asks for its `--version`"
            )
        };
        assert_eq!(
            with("requires: [jq, ' ', 'docker run --rm -v /:/host alpine true', 'python3.12 -c print(1)', /usr/bin/jq, '-rf']"),
            [
                not_a_name(1, " "),
                not_a_name(2, "docker run --rm -v /:/host alpine true"),
                not_a_name(3, "python3.12 -c print(1)"),
                not_a_name(4, "/usr/bin/jq"),
                not_a_name(5, "-rf"),
            ]
        );
    }
}
