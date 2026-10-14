//! A machine's `files` as Compose configs: one top-level config per file, named
//! `<machine>-file-<n>` (its index in the spec), and the service's mount of it. Text is a
//! `content` config, copied into the container with the owner and mode the mount gives; a
//! project file is a `file` config, mounted as it is (the plan refuses an owner or mode for it,
//! which Compose would ignore).

use indexmap::IndexMap;
use isoloom_plan::{Environment, MachinePlan, MachineShape};

use crate::docker::ROOT;
use crate::file::{Config, ConfigMount};

/// The name of machine `machine`'s `n`th file's config.
fn name(machine: &str, n: usize) -> String {
    format!("{machine}-file-{n}")
}

/// Every container machine's files, as top-level configs.
pub(super) fn configs(env: &Environment) -> IndexMap<String, Config> {
    env.machines
        .iter()
        .filter(|(_, mp)| mp.shape == MachineShape::Container)
        .flat_map(|(machine, mp)| {
            mp.files.iter().enumerate().map(move |(n, f)| {
                let config = match (&f.source, &f.content) {
                    (Some(source), _) => Config {
                        file: Some(format!("{ROOT}/{source}")),
                        content: None,
                    },
                    (None, content) => Config {
                        file: None,
                        content: Some(content.as_deref().unwrap_or_default().replace('$', "$$")),
                    },
                };
                (name(machine, n), config)
            })
        })
        .collect()
}

/// Where machine `machine`'s service has its files: a text with its owner (when the spec sets
/// one) and its mode, a project file as it is.
pub(super) fn mounts(machine: &str, mp: &MachinePlan) -> Vec<ConfigMount> {
    mp.files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            let text = f.source.is_none();
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
}
