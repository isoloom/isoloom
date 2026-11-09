//! The containers' host folders (`host_folders`): each bound from where Compose finds it. On this
//! computer's Docker that is the folder itself; Compose in a VM (`docker-vm`) finds it where the
//! VM's shared folder puts it, which the commands run there say through one variable per folder,
//! so both run the same Compose file.

use isoloom_plan::{Environment, MachinePlan, MachineShape};
use isoloom_spec::model::{HostFolder, HostPath};

/// Where Compose in a VM finds the host folders: `<VM_ROOT>/<machine>/<n>`.
pub const VM_ROOT: &str = "/mnt/isoloom";

/// One container's host folder: whose, which, and its names on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerFolder<'a> {
    /// The machine.
    pub machine: &'a str,
    /// The folder, as the spec gives it.
    pub folder: &'a HostFolder,
    /// The variable of Compose's environment naming it in a VM ([`variable`]).
    pub variable: String,
    /// Where the VM's shared folder puts it.
    pub in_vm: String,
}

/// The variable of Compose's environment holding where machine `machine`'s `n`th host folder is,
/// when Compose runs in a VM (`ISOLOOM_HOST_FOLDER_WEB_0`). Machine names are kebab-case, so two
/// folders never share one.
#[must_use]
pub fn variable(machine: &str, n: usize) -> String {
    format!("ISOLOOM_HOST_FOLDER_{}_{n}", machine.to_ascii_uppercase().replace('-', "_"))
}

/// Where Compose on this computer finds `f`, from its file `root` (the project folder, relative
/// to the Compose file): a project folder under `root`, a home one as `~` (Compose expands it),
/// an absolute one as it is.
#[must_use]
pub fn on_this_computer(f: &HostFolder, root: &str) -> String {
    match f.host() {
        HostPath::Project(p) => format!("{root}/{}", p.trim_start_matches("./")),
        HostPath::Home(p) | HostPath::Absolute(p) => p.to_owned(),
    }
}

/// The host folders of the containers of `env`, machine by machine in the spec's order.
#[must_use]
pub fn of_containers(env: &Environment) -> Vec<ContainerFolder<'_>> {
    containers_with_folders(env)
        .flat_map(|(name, m)| {
            m.host_folders.iter().enumerate().map(move |(n, folder)| ContainerFolder {
                machine: name,
                folder,
                variable: variable(name, n),
                in_vm: format!("{VM_ROOT}/{name}/{n}"),
            })
        })
        .collect()
}

/// The first container of `env` that mounts a folder of the computer that runs the lab, if one
/// does: the VM its Compose runs in (`docker-vm`, `cloud-docker`) must then run on that computer,
/// to share the folder into it.
#[must_use]
pub fn needing_a_local_vm(env: &Environment) -> Option<&str> {
    containers_with_folders(env).map(|(name, _)| name).next()
}

/// The containers of `env` with a host folder, in the spec's order.
fn containers_with_folders(env: &Environment) -> impl Iterator<Item = (&str, &MachinePlan)> {
    env.machines
        .iter()
        .filter(|(_, m)| m.shape == MachineShape::Container && !m.host_folders.is_empty())
        .map(|(name, m)| (name.as_str(), m))
}

#[cfg(test)]
mod tests {
    use isoloom_spec::model::Target;

    use super::*;

    fn folder(from: &str) -> HostFolder {
        HostFolder {
            from: from.into(),
            path: "/x".into(),
            read_only: false,
        }
    }

    #[test]
    fn compose_on_this_computer_reads_a_project_folder_from_its_root_and_the_others_as_written() {
        let read = ["shared", "./shared", "../up", "~/notes", "/srv/share", "C:\\share"].map(|f| on_this_computer(&folder(f), "../.."));
        assert_eq!(read, ["../../shared", "../../shared", "../../../up", "~/notes", "/srv/share", "C:\\share"]);
    }

    #[test]
    fn each_containers_folder_has_its_own_variable_and_place_in_the_vm() {
        let env = crate::testing::env(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines:\n  web-1: { networks: { lan: 10 }, app: { image: nginx }, host_folders: [{ from: a, path: /a }, { from: b, path: /b }] }\n  db: { networks: { lan: 11 }, app: { image: redis }, host_folders: [{ from: c, path: /c }] }\n",
            Target::Docker,
        );
        let got: Vec<(&str, String, String)> = of_containers(&env).into_iter().map(|f| (f.machine, f.variable, f.in_vm)).collect();
        assert_eq!(
            got,
            [
                ("web-1", "ISOLOOM_HOST_FOLDER_WEB_1_0".into(), "/mnt/isoloom/web-1/0".into()),
                ("web-1", "ISOLOOM_HOST_FOLDER_WEB_1_1".into(), "/mnt/isoloom/web-1/1".into()),
                ("db", "ISOLOOM_HOST_FOLDER_DB_0".into(), "/mnt/isoloom/db/0".into()),
            ]
        );
        assert_eq!(needing_a_local_vm(&env), Some("web-1"));
    }

    #[test]
    fn containers_without_a_host_folder_run_on_any_vm() {
        let env = crate::testing::env(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.0.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
            Target::Docker,
        );
        assert_eq!(needing_a_local_vm(&env), None);
    }
}
