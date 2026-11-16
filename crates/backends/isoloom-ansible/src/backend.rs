//! The `external` target as a backend: what hosts that already exist can run, and the files
//! that reach them.

use isoloom_plan::{Capabilities, Environment, Feature, MachinePlan};
use isoloom_render::Error as RenderError;
use isoloom_render::backend::{Backend, Files};
use isoloom_render::inventory::{self, Access, Vars};
use isoloom_render::{GeneratedFile, header};
use isoloom_spec::contract::OUTPUT_DIR;
use isoloom_spec::model::Target;

use crate::checks;
use crate::error::{Error, Result};

/// The target's folder, under `.isoloom/`.
const DIR: &str = "external";

/// The `external` target's backend: the inventory, the playbooks and the check runners
/// of an environment on hosts that already exist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ExternalBackend;

impl Backend for ExternalBackend {
    fn target(&self) -> Target {
        Target::External
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn render(&self, env: &Environment) -> std::result::Result<Files, RenderError> {
        if let Some(what) = windows_secrets(env) {
            return Err(RenderError::Unsupported {
                target: Target::External,
                what,
            });
        }
        Ok(render(env)?)
    }

    /// The plan's refusal, then a Windows host's secret files.
    fn refusal(&self, env: &Environment) -> Option<String> {
        isoloom_plan::refusal(&env.requirements, &capabilities())
            .map(|r| r.to_string())
            .or_else(|| windows_secrets(env))
    }
}

/// A Windows host that gets secret files: the play writes them with `ansible.builtin.copy`,
/// which runs on Linux and other POSIX hosts only.
fn windows_secrets(env: &Environment) -> Option<String> {
    let (name, _) = env.machines.iter().find(|(_, m)| m.is_windows() && !m.secrets.is_empty())?;
    Some(format!(
        "machine `{name}` is Windows and gets secret files, which the `external` target writes with `ansible.builtin.copy` on Linux hosts only"
    ))
}

/// What hosts that already exist can run. Provisioning runs from the project with the
/// inventory; checks are probes from each machine (a playbook check needs a controller, which
/// Isoloom adds none of, so it is refused). A gateway machine is one of the existing hosts,
/// routing as it already does. Appliances need Docker's networks, which existing hosts don't
/// have.
fn capabilities() -> Capabilities {
    Capabilities::of([
        Feature::EnvironmentProvisioning,
        Feature::ExecWithoutContainer,
        Feature::Gateways,
        Feature::MachineAliases,
        Feature::Ulimits,
        Feature::TrafficControl,
        // The hosts exist with their addresses: an IPv6 network adds nothing to build. An
        // IPv6-only one would need their IPv6 addresses, which the spec doesn't give.
        Feature::Ipv6,
        Feature::Ipv6Auto,
        // The hosts are already on their networks, outside ones included.
        Feature::ExternalNetworks,
        Feature::BridgedNetworks,
        // Secret files: `site.yml` delivers each from where `ansible-playbook` runs, a lookup of
        // its source, never its value, in the generated files; as the owner the spec gives.
        Feature::Secrets,
        Feature::SecretOwners,
    ])
}

/// The files of `env` (planned for [`Target::External`]): the inventory, then a check runner per
/// host with probes to run, then the playbooks. Refused, without rendering,
/// when `env` needs what existing hosts can't give.
fn render(env: &Environment) -> Result<Files> {
    if let Some(refusal) = isoloom_plan::refusal(&env.requirements, &capabilities()) {
        return Err(Error::Refused { refusal });
    }
    let inventory = format!("{}{}", header("#"), inventory::render(env, &access()));
    let mut files = vec![output("inventory.ini", inventory)];
    files.extend(checks::runners(env).into_iter().map(|(path, script)| output(&path, script)));
    // The Ansible CM project (`site.yml`, the machine roles and `requirements.yml`) that
    // provisions the existing hosts: the recipe's steps as native tasks, run over this inventory.
    files.extend(crate::configure::project(env, &format!("{OUTPUT_DIR}/{DIR}"))?);
    Ok(files)
}

/// How Ansible reaches the existing hosts: each one's SSH user, port and key as its `external:`
/// gives them; Windows hosts over SSH with PowerShell (OpenSSH on Windows), not WinRM.
pub(crate) fn access() -> Access<'static> {
    Access::new(Vars::new(), inventory::vars(&inventory::WINDOWS_OVER_SSH), |h| external_vars(h.machine))
}

/// An existing host's `ansible_user`, `ansible_port` and key: the ones its `external:` sets.
fn external_vars(m: &MachinePlan) -> Vars {
    let Some(e) = &m.external else { return Vars::new() };
    let mut out = Vars::new();
    if let Some(u) = &e.user {
        out.push(("ansible_user".into(), u.clone()));
    }
    if let Some(p) = e.port {
        out.push(("ansible_port".into(), p.to_string()));
    }
    if let Some(k) = &e.key {
        out.push(("ansible_ssh_private_key_file".into(), k.clone()));
    }
    out
}

/// The file at `path` in the target's folder.
fn output(path: &str, contents: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{OUTPUT_DIR}/{DIR}/{path}"),
        contents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::environment;

    #[test]
    fn an_appliance_is_refused_from_the_capabilities_without_rendering() {
        let env = environment(
            "  r: { networks: { lab: 1 }, app: { image: x, appliance: cisco-iol } }\noverrides: { external: { machines: { r: { ssh: { address: 10.0.0.1 } } } } }\n",
        );
        let refusal = ExternalBackend.refusal(&env).expect("refused");
        assert!(refusal.contains("network appliance"), "{refusal}");
        assert!(matches!(ExternalBackend.render(&env), Err(RenderError::Refused { .. })));
    }

    #[test]
    fn a_check_an_existing_host_cant_run_is_refused_without_rendering() {
        let hosts = "overrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n";
        for (checks, why) in [
            ("[checks/own.sh]", "check \"checks/own.sh\" runs a project script"),
            ("[{ name: own, from: a, script: checks/own.sh }]", "check \"own\" runs a project script"),
            ("[checks/own.yml]", "Ansible checks (.yml)"),
            ("[{ name: web, tcp: 'a:80' }]", "check \"web\" has no host to run from"),
        ] {
            let env = environment(&format!("  a: {{ networks: {{ lab: 5 }}, os: debian@12 }}\n{hosts}checks: {checks}\n"));
            let refusal = ExternalBackend.refusal(&env).unwrap_or_else(|| panic!("{checks}: not refused"));
            assert!(refusal.contains(why), "{checks}: {refusal}");
            assert!(matches!(ExternalBackend.render(&env), Err(RenderError::Refused { .. })), "{checks}");
        }
    }

    #[test]
    fn an_existing_hosts_own_settings_are_refused_not_dropped() {
        let hosts = "overrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n";
        for (settings, field) in [
            ("environment: { MODE: lab }", "environment"),
            ("sysctls: { net.core.somaxconn: '4096' }", "sysctls"),
            ("ulimits: { nofile: 1024 }", "ulimits"),
        ] {
            let env = environment(&format!("  a: {{ networks: {{ lab: 5 }}, os: debian@12, {settings} }}\n{hosts}"));
            let refusal = ExternalBackend.refusal(&env).unwrap_or_default();
            assert!(
                refusal.contains(&format!("machine `a` sets `{field}`: Isoloom doesn't set up a host that already exists")),
                "{settings}: {refusal}"
            );
            assert!(matches!(ExternalBackend.render(&env), Err(RenderError::Refused { .. })), "{settings}");
        }
    }

    /// A secret file reaches an existing host through `site.yml`: its value read where
    /// `ansible-playbook` runs (an environment variable, a file, `vault`, `sops`), written at its
    /// path with its owner and mode, the task out of the log; the files hold the lookup, never
    /// the value.
    #[test]
    fn a_secret_file_is_delivered_by_a_lookup_where_ansible_runs() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12, files: [{ path: /run/secrets/api-key, secret: api-key, uid: 33, mode: '0440' }, { path: /run/lab/token, input: TOKEN }, { path: /etc/db, secret: db }, { path: /etc/s, secret: s }] }\ninputs: { TOKEN: { sensitive: true } }\nsecrets: { api-key: { env: API_KEY }, db: { vault: 'secret/app#db_password' }, s: { sops: 'secrets.enc.yaml#db.password' } }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n",
        );
        assert_eq!(ExternalBackend.refusal(&env), None);
        let files = render(&env).expect("renders");
        let site = files.iter().find(|f| f.path.ends_with("external/site.yml")).expect("site.yml");
        let plays: serde_yaml_ng::Value = serde_yaml_ng::from_str(&site.contents).expect("YAML");
        let tasks = &plays[0]["tasks"];
        assert_eq!(plays[0]["name"], "Deliver a's secrets");
        let copies: Vec<&serde_yaml_ng::Value> = tasks
            .as_sequence()
            .expect("tasks")
            .iter()
            .filter(|t| t.get("ansible.builtin.copy").is_some())
            .collect();
        assert_eq!(copies.len(), 4);
        let key = copies[0];
        assert_eq!(key["no_log"], true);
        assert_eq!(key["ansible.builtin.copy"]["dest"], "/run/secrets/api-key");
        assert_eq!(key["ansible.builtin.copy"]["owner"], "33");
        assert_eq!(key["ansible.builtin.copy"]["mode"], "0440");
        assert_eq!(
            key["ansible.builtin.copy"]["content"],
            "{{ lookup('ansible.builtin.env', isoloom_secret_env) }}"
        );
        assert_eq!(key["vars"]["isoloom_secret_env"], "API_KEY");
        assert_eq!(copies[1]["vars"]["isoloom_secret_env"], "TOKEN");
        assert_eq!(copies[1]["ansible.builtin.copy"]["mode"], "0400");
        assert_eq!(copies[2]["vars"]["isoloom_secret_command"], "vault kv get -field='db_password' 'secret/app'");
        assert_eq!(
            copies[3]["vars"]["isoloom_secret_command"],
            "sops -d --extract '[\"db\"][\"password\"]' {project}/'secrets.enc.yaml'"
        );
        assert!(!site.contents.contains("db_password\n"), "{}", site.contents);
    }

    /// A secret kept in a file of the computer `ansible-playbook` runs on is read there: a
    /// relative path from the project folder, `~/…` from the home folder, an absolute one as it
    /// is.
    #[test]
    fn a_secret_from_a_file_is_read_where_ansible_runs() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12, files: [{ path: /etc/p, secret: p }, { path: /etc/h, secret: h }, { path: /etc/r, secret: r }] }\nsecrets: { p: { file: keys/p.txt }, h: { file: '~/keys/h.txt' }, r: { file: /srv/keys/r.txt } }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n",
        );
        let files = render(&env).expect("renders");
        let site = files.iter().find(|f| f.path.ends_with("external/site.yml")).expect("site.yml");
        let plays: serde_yaml_ng::Value = serde_yaml_ng::from_str(&site.contents).expect("YAML");
        let read: Vec<(&str, &str, &str)> = plays[0]["tasks"]
            .as_sequence()
            .expect("tasks")
            .iter()
            .filter_map(|t| {
                let copy = t.get("ansible.builtin.copy")?;
                Some((copy["dest"].as_str()?, copy["content"].as_str()?, t["vars"]["isoloom_secret_file"].as_str()?))
            })
            .collect();
        assert_eq!(
            read,
            [
                (
                    "/etc/p",
                    "{{ lookup('ansible.builtin.file', isoloom_project ~ '/' ~ isoloom_secret_file) }}",
                    "keys/p.txt"
                ),
                (
                    "/etc/h",
                    "{{ lookup('ansible.builtin.file', lookup('ansible.builtin.env', 'HOME') ~ '/' ~ isoloom_secret_file) }}",
                    "keys/h.txt"
                ),
                ("/etc/r", "{{ lookup('ansible.builtin.file', isoloom_secret_file) }}", "/srv/keys/r.txt"),
            ],
            "{}",
            site.contents
        );
    }

    #[test]
    fn a_windows_hosts_secret_file_is_refused_not_dropped() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: windows-server@2022, files: [{ path: 'C:\\secrets\\key', secret: key }] }\nsecrets: { key: { env: KEY } }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n",
        );
        let refusal = ExternalBackend.refusal(&env).unwrap_or_default();
        assert!(refusal.contains("machine `a` is Windows and gets secret files"), "{refusal}");
        assert!(matches!(ExternalBackend.render(&env), Err(RenderError::Unsupported { .. })));
    }

    #[test]
    fn a_step_gets_the_machines_inputs_from_where_ansible_runs() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12, environment: { MODE: { input: MODE } }, app: { steps: [{ run: setup.sh }, { playbook: site.yml }] } }\ninputs: { MODE: { sensitive: false } }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } } } } }\n",
        );
        let files = render(&env).expect("renders");
        let tasks = files
            .iter()
            .find(|f| f.path.ends_with("roles/isoloom_machine_a/tasks/main.yml"))
            .expect("a's role");
        let tasks: serde_yaml_ng::Value = serde_yaml_ng::from_str(&tasks.contents).expect("the tasks read");
        let steps: Vec<&serde_yaml_ng::Value> = tasks
            .as_sequence()
            .expect("a list of tasks")
            .iter()
            .filter(|t| t.get("ansible.builtin.script").is_some() || t.get("ansible.builtin.command").is_some())
            .collect();
        assert_eq!(steps.len(), 2, "{}", tasks_text(&files));
        for step in steps {
            assert_eq!(
                step["environment"]["MODE"].as_str(),
                Some("{{ lookup('ansible.builtin.env', 'MODE') }}"),
                "{}",
                tasks_text(&files)
            );
        }
    }

    /// Every generated file's text, for a failing assertion.
    fn tasks_text(files: &[GeneratedFile]) -> String {
        files.iter().map(|f| ["# ", &f.path, "\n", &f.contents].concat()).collect()
    }

    #[test]
    fn rendering_twice_gives_the_same_files() {
        let env = environment(
            "  a: { networks: { lab: 5 }, os: debian@12 }\n  b: { networks: { lab: 6 }, os: debian@12 }\noverrides: { external: { machines: { a: { ssh: { address: 10.0.0.5 } }, b: { ssh: { address: 10.0.0.6 } } } } }\n",
        );
        assert_eq!(render(&env).expect("renders"), render(&env).expect("renders again"));
    }
}

/// The inventory as the existing hosts are reached.
#[cfg(test)]
mod inventory_tests {
    use isoloom_render::inventory::render;
    use isoloom_render::test_support::{INVENTORY_SPEC as SPEC, example, planned, section};
    use isoloom_spec::model::Target;

    use super::access;

    #[test]
    fn external_hosts_carry_their_user_port_and_quoted_key() {
        let inv = render(&planned(SPEC, Target::External).unwrap(), &access());
        assert!(
            inv.starts_with("[linux]\nweb ansible_host=web.example.org ansible_user=ops ansible_port=2222 ansible_ssh_private_key_file='~/my keys/id' code="),
            "{inv}"
        );
        assert_eq!(section(&inv, "windows"), "dc ansible_host=192.168.1.20\n\n");
        assert_eq!(section(&inv, "windows:vars"), "ansible_connection=ssh\nansible_shell_type=powershell\n\n");
        assert_eq!(section(&inv, "linux:vars"), "ansible_become=true\n\n");
    }

    #[test]
    fn external_existing_hosts() {
        insta::assert_snapshot!(render(&example("existing-hosts", Target::External).unwrap(), &access()));
    }
}
