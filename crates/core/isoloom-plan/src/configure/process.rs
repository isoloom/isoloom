//! What a container runs (design §3): one daemon as its foreground process, several under the
//! engine's init and Isoloom's runner, the machine's own command, or nothing (idle). The one rule
//! for a container built from steps: a daemon runs only where a `service` step or `command` says
//! so, and a container that would answer nothing on its ports is refused, as is a daemon with no
//! known foreground form.

use isoloom_spec::model::CommandLine;
use isoloom_spec::syntax::{self, UnitName};

use super::daemons::Daemon;
use super::recipe::Recipe;
use super::resources::Resource;

/// The process a container runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerProcess {
    /// The official image's own entrypoint and command (a catalogue entry's image form).
    Image,
    /// A catalogue entry's official image whose `start` steps Isoloom's runner runs at each
    /// start (`image.start: run`), then `exec`s the image's own entrypoint with its command.
    Started {
        /// The image's entrypoint (empty: its command alone).
        entrypoint: Vec<String>,
        /// The command it runs (the image's `CMD`, or the entry's).
        command: Vec<String>,
    },
    /// Nothing of its own: the container is kept running idle, a machine to work from.
    Idle,
    /// The machine's `command`, as the container's command.
    Command(CommandLine),
    /// One daemon: the runner runs `start`, then `exec`s the daemon's foreground form.
    One(Daemon),
    /// Several daemons (or one and the command) under the engine's init (tini): the runner runs
    /// `start`, starts each in order, and exits when one does.
    Supervised {
        /// The daemons, in recipe order.
        daemons: Vec<Daemon>,
        /// The machine's command, started after them.
        command: Option<CommandLine>,
    },
}

/// Why a container built from a recipe can't run it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoProcess {
    /// An enabled service with no foreground form: neither its own `run` nor the daemon table's.
    NoForeground {
        /// The service, as its step names it.
        unit: UnitName,
    },
    /// The machine has ports and steps, but nothing that serves them runs: the container would
    /// build and answer nothing.
    NothingServes,
}

impl ContainerProcess {
    /// What machine `m`'s container runs, its recipe resolved as `recipe`.
    ///
    /// # Errors
    ///
    /// [`NoProcess`]: a service the container can't run, or ports nothing serves.
    pub fn of(recipe: &Recipe, m: &syntax::Machine) -> Result<ContainerProcess, NoProcess> {
        ContainerProcess::resolve(recipe, m.command.as_ref().map(|c| &c.run), !m.ports.is_empty())
    }

    /// What a container runs, its recipe resolved as `recipe`, with `command` (the machine's own)
    /// and ports (`ports`: whether it answers on any).
    ///
    /// # Errors
    ///
    /// [`NoProcess`]: a service the container can't run, or ports nothing serves.
    pub fn resolve(recipe: &Recipe, command: Option<&CommandLine>, ports: bool) -> Result<ContainerProcess, NoProcess> {
        let mut daemons = Vec::new();
        for r in recipe.resources() {
            let Resource::Service(s) = r else { continue };
            if !s.enabled {
                continue;
            }
            match recipe.daemons.iter().find(|d| d.unit == s.unit) {
                Some(d) => daemons.push(d.clone()),
                None => return Err(NoProcess::NoForeground { unit: s.unit.clone() }),
            }
        }
        match (daemons.len(), command.cloned()) {
            (0, None) if ports && recipe.resources().next().is_some() => Err(NoProcess::NothingServes),
            (0, None) => Ok(ContainerProcess::Idle),
            (0, Some(c)) => Ok(ContainerProcess::Command(c)),
            (1, None) => Ok(daemons.pop().map_or(ContainerProcess::Idle, ContainerProcess::One)),
            (_, command) => Ok(ContainerProcess::Supervised { daemons, command }),
        }
    }

    /// Whether Isoloom's runner is the container's process (one daemon, or several): it needs
    /// the engine's init (Compose's `init: true`, a pod sharing its process namespace).
    #[must_use]
    pub fn is_runner(&self) -> bool {
        matches!(
            self,
            ContainerProcess::One(_) | ContainerProcess::Supervised { .. } | ContainerProcess::Started { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DaemonTable, recipe};
    use super::*;

    /// What machine `m` (YAML) of a lab of its own runs in a container.
    fn process(yaml: &str) -> Result<ContainerProcess, NoProcess> {
        let m: syntax::Machine = serde_yaml_ng::from_str(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"));
        let os = m.os.expect("the test's machine has an os");
        let r = recipe(os, &m, &DaemonTable::common()).expect("a portable recipe");
        ContainerProcess::of(&r, &m)
    }

    #[test]
    fn no_service_and_no_command_is_idle() {
        assert_eq!(process("{ os: debian@12, app: { steps: [{ packages: [curl] }] } }"), Ok(ContainerProcess::Idle));
    }

    #[test]
    fn a_command_alone_is_the_containers_command() {
        let p = process("{ os: debian@12, ports: { web: 80 }, command: [nginx, -g, 'daemon off;'], app: { steps: [{ packages: [nginx] }] } }");
        assert!(matches!(p, Ok(ContainerProcess::Command(_))), "{p:?}");
    }

    #[test]
    fn one_enabled_service_is_the_foreground_process() {
        let Ok(ContainerProcess::One(d)) = process("{ os: debian@12, ports: { web: 80 }, app: { steps: [{ packages: [nginx] }, { service: nginx }] } }") else {
            panic!()
        };
        assert_eq!(d.foreground.args(), ["nginx", "-g", "daemon off;"]);
    }

    #[test]
    fn two_services_or_one_and_a_command_are_supervised() {
        let two = process(
            "{ os: debian@12, app: { steps: [{ service: { name: api, run: [/srv/api] } }, { service: { name: worker, run: [/srv/worker], user: api } }] } }",
        );
        let Ok(ContainerProcess::Supervised { daemons, command: None }) = two else {
            panic!("{two:?}")
        };
        assert_eq!(daemons.iter().map(|d| d.unit.as_str()).collect::<Vec<_>>(), ["api", "worker"]);
        let with_command = process("{ os: debian@12, command: [tail, -f, /dev/null], app: { steps: [{ service: { name: api, run: [/srv/api] } }] } }");
        assert!(matches!(with_command, Ok(ContainerProcess::Supervised { command: Some(_), .. })));
    }

    #[test]
    fn a_disabled_service_is_not_started() {
        let p = process("{ os: debian@12, app: { steps: [{ packages: [nginx] }, { service: { name: nginx, enabled: false } }] } }");
        assert_eq!(p, Ok(ContainerProcess::Idle));
    }

    #[test]
    fn a_service_with_no_foreground_form_is_refused() {
        let p = process("{ os: debian@12, app: { steps: [{ service: myd }] } }");
        assert_eq!(p, Err(NoProcess::NoForeground { unit: "myd".parse().unwrap() }));
        // Alpine's cron is BusyBox's: the table has no form of it there.
        assert!(process("{ os: alpine, app: { steps: [{ service: cron }] } }").is_err());
    }

    /// The rule of `4bfd8d5f`, now the model's: steps that install a service and ports, with
    /// nothing that runs it, answer nothing in a container.
    #[test]
    fn ports_nothing_serves_are_refused() {
        assert_eq!(
            process("{ os: debian@12, ports: { web: 80 }, app: { steps: [{ run: web.sh }] } }"),
            Err(NoProcess::NothingServes)
        );
        assert_eq!(
            process("{ os: debian@12, ports: { web: 80 } }"),
            Ok(ContainerProcess::Idle),
            "the OS alone stays idle, as before"
        );
    }
}
