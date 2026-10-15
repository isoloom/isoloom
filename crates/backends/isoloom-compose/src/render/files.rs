//! A machine's `files` as Compose configs: one top-level config per file, named
//! `<machine>-file-<n>` (its index in the spec), and the service's mount of it. Text is a
//! `content` config, copied into the container with the owner and mode the mount gives. A
//! project file is a `file` config, mounted as it is, unless the spec gives it an owner or a
//! mode: outside Swarm, Compose ignores both for a `file` config (it bind-mounts the file), so
//! that one is an `environment` config instead, copied in like text. Whoever starts Compose
//! reads the file from the project into the variable first ([`project_variables`]): the
//! generated file never holds the project's contents, and an edit takes effect at the next run.

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_render::container::copied_from_project;
pub use isoloom_render::container::{host_environment, project_variables, variable};

use crate::docker::ROOT;
use crate::file::{Config, ConfigMount};

/// The name of machine `machine`'s `n`th file's config.
fn name(machine: &str, n: usize) -> String {
    format!("{machine}-file-{n}")
}

/// The usage line saying where the project files with an owner or a mode come from, when the
/// file has any.
pub(super) fn usage(env: &Environment) -> String {
    let vars = project_variables(env);
    if vars.is_empty() {
        return String::new();
    }
    let each: Vec<String> = vars.iter().map(|(var, path)| format!("{var} to {path}'s contents")).collect();
    format!(
        "# Project files with an owner or a mode: `isoloom run` reads them into Compose's environment; by hand, set {} for `up`.\n",
        each.join(", ")
    )
}

/// Every container machine's files, as top-level configs.
pub(super) fn configs(env: &Environment) -> IndexMap<String, Config> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .flat_map(|(machine, mp)| {
            mp.files.iter().enumerate().map(move |(n, f)| {
                let config = match (&f.source, &f.content) {
                    (Some(_), _) if copied_from_project(f) => Config {
                        environment: Some(variable(machine, n)),
                        ..Config::default()
                    },
                    (Some(source), _) => Config {
                        file: Some(format!("{ROOT}/{source}")),
                        ..Config::default()
                    },
                    (None, content) => Config {
                        content: Some(content.as_deref().unwrap_or_default().replace('$', "$$")),
                        ..Config::default()
                    },
                };
                (name(machine, n), config)
            })
        })
        .collect()
}

/// Where machine `machine`'s service has its files: a text, or a project file with an owner or
/// a mode, with its owner (when the spec sets one) and its mode; any other project file as it
/// is.
pub(super) fn mounts(machine: &str, mp: &MachinePlan) -> Vec<ConfigMount> {
    mp.files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            let text = f.source.is_none() || copied_from_project(f);
            ConfigMount {
                source: name(machine, n),
                target: f.path.clone(),
                uid: f.uid.filter(|_| text).map(|u| u.to_string()),
                gid: f.gid.filter(|_| text).map(|g| g.to_string()),
                mode: text.then(|| f.mode_or_default().to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use isoloom_plan::Environment;
    use isoloom_spec::model::Target;

    const SPEC: &str = "
version: 1
name: files
networks:
  lan: { cidr: 10.20.0.0/24 }
machines:
  web:
    networks: { lan: 10 }
    docker: { image: nginx }
    files:
      - { path: /etc/app.ini, content: \"home=$HOME\\n\", uid: 101, gid: 102, mode: '0600' }
      - { path: /etc/nginx/conf.d/site.conf, source: conf/site.conf }
      - { path: /etc/app/key.pem, source: conf/key.pem, uid: 101, mode: '0400' }
";

    #[test]
    fn text_is_copied_with_its_owner_and_a_project_file_mounted() {
        let spec = isoloom_spec::parse(SPEC).expect("parses");
        let env = Environment::build(&spec, Target::Docker, None);
        let configs = super::configs(&env);
        assert_eq!(configs["web-file-0"].content.as_deref(), Some("home=$$HOME\n"));
        assert_eq!(configs["web-file-1"].file.as_deref(), Some("../../conf/site.conf"));
        let mounts = super::mounts("web", &env.machines["web"]);
        assert_eq!(
            (mounts[0].uid.as_deref(), mounts[0].gid.as_deref(), mounts[0].mode.as_deref()),
            (Some("101"), Some("102"), Some("0600"))
        );
        assert_eq!((mounts[1].target.as_str(), mounts[1].mode.as_deref()), ("/etc/nginx/conf.d/site.conf", None));
    }

    #[test]
    fn an_owned_project_file_is_copied_in_from_compose_s_environment() {
        let spec = isoloom_spec::parse(SPEC).expect("parses");
        let env = Environment::build(&spec, Target::Docker, None);
        let configs = super::configs(&env);
        assert_eq!(configs["web-file-2"].environment.as_deref(), Some("ISOLOOM_FILE_WEB_2"));
        assert_eq!((configs["web-file-2"].file.as_ref(), configs["web-file-2"].content.as_ref()), (None, None));
        let mounts = super::mounts("web", &env.machines["web"]);
        assert_eq!(
            (mounts[2].uid.as_deref(), mounts[2].gid.as_deref(), mounts[2].mode.as_deref()),
            (Some("101"), None, Some("0400"))
        );
        assert_eq!(super::project_variables(&env), [("ISOLOOM_FILE_WEB_2".to_string(), "conf/key.pem".to_string())]);
        assert_eq!(super::variable("db-main", 0), "ISOLOOM_FILE_DB_MAIN_0");
        assert_eq!(
            super::host_environment(&env),
            "ISOLOOM_FILE_WEB_2=\"$(cat 'conf/key.pem'; printf x)\" && ISOLOOM_FILE_WEB_2=\"${ISOLOOM_FILE_WEB_2%x}\" && export ISOLOOM_FILE_WEB_2 && "
        );
        assert!(
            super::usage(&env).contains("set ISOLOOM_FILE_WEB_2 to conf/key.pem's contents"),
            "{}",
            super::usage(&env)
        );
    }
}
