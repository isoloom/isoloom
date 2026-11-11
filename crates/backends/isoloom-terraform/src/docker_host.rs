//! The Docker host: one VM running the environment's containers with the `docker` target's
//! Compose file, on a cloud (`cloud-docker`) or a Proxmox server (`docker-vm`). Every module of
//! it shares its size (`isoloom_render::container::HostSize`) and the ports it opens; a cloud's
//! host sets itself up from its user data (`crate::cloud::docker::setup`), and the Proxmox one
//! through the docker-vm run manifest. Either is readied the same way over SSH ([`into_vm`]): the
//! project's archive piped in, then [`setup_script`] (Docker); then the Compose backend's
//! commands run through the same way in ([`Login`]).

use isoloom_manifest::{Error as ManifestError, Failure, Machine, Output, OutputRole, PathTemplate, RelPath, Step, StepId, Template};
use isoloom_render::run_manifest::{self, guest::COMPOSE_OUTPUTS, guest::GuestCommand, guest::GuestCompose};
use isoloom_render::steps::{self, AsRoot, CLOUD_INIT_WAIT, PUBLISHED_FROM_OUTSIDE, install_docker};

use crate::hcl_builder::{Expression, list, string};
use crate::ssh::Login;
use isoloom_plan::{Environment, MachineShape};
use isoloom_spec::model::Protocol;

/// The ports the containers publish over `protocol`, sorted, each once.
pub(crate) fn published(env: &Environment, protocol: Protocol) -> Vec<u16> {
    let mut ports: Vec<u16> = env
        .machines
        .values()
        .filter(|m| m.shape == MachineShape::Container)
        .flat_map(|m| &m.ports)
        .filter(|p| p.protocol == protocol)
        .map(|p| p.publish)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// SSH, then the ports the containers publish over TCP: what a firewall taking one list of TCP
/// ports opens.
pub(crate) fn tcp_ports(env: &Environment) -> Vec<u16> {
    let mut tcp = vec![22];
    tcp.extend(published(env, Protocol::Tcp));
    tcp
}

/// The ports as strings (`["22", "8080"]`), for the firewalls that take them so.
pub(crate) fn port_strings(ports: &[u16]) -> Expression {
    list(ports.iter().map(|p| string(p.to_string())))
}

/// The command that makes a VM readied over SSH accept the [`GuestCompose::forwarded`]
/// variables (`AcceptEnv`) and reloads sshd, run before the start steps so the sessions that
/// create the containers carry the launch's values. `None` when nothing is forwarded. A VM
/// whose image sets `AcceptEnv` from cloud-init (docker-vm on Proxmox) does not need it; a VM
/// Terraform makes bare (cloud-docker) does, since nothing else tells its sshd. Run as root as
/// `as_root` says.
///
/// # Errors
///
/// A step id the manifest refuses (a bug: it is fixed). The forwarded names are environment
/// variables (upper snake case), so the command holds no single quote to escape.
pub(crate) fn accept_env(guest: &GuestCompose, as_root: AsRoot) -> Result<Option<GuestCommand>, ManifestError> {
    let forwarded = guest.forwarded();
    if forwarded.is_empty() {
        return Ok(None);
    }
    Ok(Some(GuestCommand {
        id: StepId::parse("accept-env")?,
        note: Some("Letting the VM accept the launch's values…".into()),
        command: Template::text(&format!("{}sh -c '{}'", as_root.prefix(), steps::accept_env(&forwarded))),
        timeout_s: 60,
        failure: Failure::Fail,
        download: false,
    }))
}

/// The outputs of a Docker host Terraform made: its address (`ip`, where SSH reaches it and the
/// published ports answer) and its SSH user (`ssh_user`), then each of `machines`' published
/// ports at that address.
///
/// # Errors
///
/// A name the manifest refuses.
pub(crate) fn docker_host_outputs(machines: &[Machine]) -> Result<Vec<Output>, ManifestError> {
    let mut out = vec![
        crate::module::manifest::terraform_output(
            "ip",
            OutputRole::Address,
            "The Docker host's address: SSH reaches it there, and its containers' published ports answer there.",
        )?,
        crate::module::manifest::terraform_output("ssh_user", OutputRole::Info, "The user the runner's key lets in on the Docker host.")?,
    ];
    out.extend(run_manifest::outputs_on_their_hosts(machines)?);
    Ok(out)
}

/// How long the set-up script may take: cloud-init's end, then Docker's install.
const SETUP_S: u32 = 1800;

/// A VM Terraform made, readied for Compose through `login`: the project archived
/// (`project.archive`, `root` its folder from the manifest's, with [`COMPOSE_OUTPUTS`]) and piped
/// into `tar` in the VM's project folder, replacing what an earlier copy left, then `setup` (the
/// generated [`setup_script`], beside the manifest) piped into `sh -s` as root, which trusts the
/// project's certificate authorities before it downloads Docker. Each command gets root as the
/// login's user does ([`AsRoot`]).
///
/// # Errors
///
/// A path, key or id the manifest refuses (a bug: they are fixed).
pub(crate) fn into_vm(login: &Login, setup: &RelPath, root: &str) -> Result<Vec<Step>, ManifestError> {
    let sudo = login.as_root().prefix();
    let way_in = login.way_in();
    let docker = GuestCommand {
        id: StepId::parse("docker")?,
        note: Some("Installing Docker in the VM…".into()),
        command: Template::text(&format!("{sudo}sh -s")),
        timeout_s: SETUP_S,
        failure: Failure::Fail,
        download: false,
    };
    let mut docker = docker.step(&way_in)?;
    docker.stdin = Some(PathTemplate::relative(setup));
    let archive = run_manifest::project_archive(root, &COMPOSE_OUTPUTS)?;
    let copy = GuestCommand {
        id: StepId::parse("project")?,
        note: Some("Copying the project into the VM…".into()),
        command: Template::text(&format!("{sudo}sh -c '{}'", steps::project_unpack(steps::Archive::Stdin, false))),
        timeout_s: 600,
        failure: Failure::Fail,
        download: false,
    };
    let mut copy = copy.step(&way_in)?;
    copy.stdin = Some(PathTemplate::project_archive());
    Ok(vec![archive, copy, docker])
}

/// The Docker host's set-up script for `env`, piped into `sh -s` as root (with `sudo` when the
/// login's user isn't root) once the project is in: once cloud-init is done, the spec's constraints (proxy, mirrors, NTP) and certificate
/// authorities, then Docker from Docker's own script unless it is there, and published ports
/// answering whatever the containers' routes ([`PUBLISHED_FROM_OUTSIDE`]). Running it again
/// changes nothing.
#[must_use]
pub(crate) fn setup_script(env: &Environment) -> String {
    let prelude = steps::linux(env, "", &isoloom_plan::Step::Prelude).unwrap_or_default();
    let trust = steps::trust_authorities(env).unwrap_or_default();
    format!(
        "#!/bin/sh\n{}# The Docker host's set-up, piped into `sh -s` as root over SSH by its run manifest.\nset -e\n{CLOUD_INIT_WAIT}\n{prelude}{trust}{}\n{PUBLISHED_FROM_OUTSIDE}",
        isoloom_render::header("#"),
        install_docker()
    )
}

#[cfg(test)]
mod tests {

    use isoloom_manifest::{Action, Run, SecretSlot};
    use isoloom_spec::model::Protocol;

    use super::*;
    use crate::cloud::docker::tests::{SPEC, env};
    use isoloom_render::test_support::slot;

    /// A guest compose carrying only `slots`, with a plain `ps` status.
    fn guest(slots: Vec<SecretSlot>) -> GuestCompose {
        let status = GuestCommand {
            id: StepId::parse("ps").unwrap(),
            note: None,
            command: Template::text("docker compose ps"),
            timeout_s: 60,
            failure: Failure::Fail,
            download: false,
        };
        GuestCompose {
            slots,
            ..isoloom_render::test_support::empty_guest(status)
        }
    }

    #[test]
    fn published_ports_are_sorted_once_per_protocol() {
        let e = env(SPEC);
        assert_eq!(published(&e, Protocol::Tcp), [8080, 15432]);
        assert_eq!(published(&e, Protocol::Udp), [5353]);
        assert_eq!(tcp_ports(&e), [22, 8080, 15432]);
    }

    #[test]
    fn a_vm_terraform_made_gets_the_project_then_docker_over_ssh() {
        let login = Login::new(Template::text("10.0.0.9"), "isoloom");
        let steps = into_vm(&login, &RelPath::parse("setup.sh").unwrap(), "../../..").unwrap();
        let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["archive", "project", "docker"]);
        assert_eq!(steps[2].stdin.as_ref().map(ToString::to_string).as_deref(), Some("setup.sh"));
        assert_eq!(
            steps[1].stdin.as_ref().map(ToString::to_string).as_deref(),
            Some("${run.state_dir}/project.tgz")
        );
        let Run::Action { action: Action::SshRun, with } = &steps[1].run else {
            panic!("{:?}", steps[1].run)
        };
        let pairs: Vec<(String, String)> = with.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(
            pairs,
            [
                (
                    "command".into(),
                    "sudo sh -c 'rm -rf /opt/isoloom && mkdir -p /opt/isoloom && tar -xzf - -C /opt/isoloom'".into()
                ),
                ("host".into(), "10.0.0.9".into()),
                ("user".into(), "isoloom".into())
            ]
        );
        let Run::Action {
            action: Action::ProjectArchive,
            with: archived,
        } = &steps[0].run
        else {
            panic!()
        };
        let archived: Vec<(String, String)> = archived.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        // The outputs Compose reads go in, no other.
        assert_eq!(
            archived,
            [
                ("outputs".into(), ".isoloom/docker .isoloom/configure".into()),
                ("root".into(), "../../..".into())
            ]
        );
    }

    #[test]
    fn a_root_login_readies_the_vm_without_sudo() {
        let login = Login::new(Template::text("10.0.0.9"), "root");
        let steps = into_vm(&login, &RelPath::parse("setup.sh").unwrap(), "../../..").unwrap();
        let commands: Vec<String> = steps[1..]
            .iter()
            .map(|s| {
                let Run::Action { with, .. } = &s.run else { panic!() };
                with.iter().find(|(k, _)| k.as_str() == "command").map(|(_, v)| v.to_string()).unwrap()
            })
            .collect();
        assert_eq!(
            commands,
            ["sh -c 'rm -rf /opt/isoloom && mkdir -p /opt/isoloom && tar -xzf - -C /opt/isoloom'", "sh -s"]
        );
        let accept = accept_env(&guest(vec![slot("TOKEN", "LAUNCH_TOKEN").unwrap()]), AsRoot::Root).unwrap().unwrap();
        assert!(!accept.command.to_string().contains("sudo"), "{}", accept.command);
    }

    #[test]
    fn the_old_copy_cleared_is_always_the_fixed_project_folder() {
        let login = Login::new(Template::text("10.0.0.9"), "isoloom");
        let steps = into_vm(&login, &RelPath::parse("setup.sh").unwrap(), "../../..").unwrap();
        let Run::Action { with, .. } = &steps[1].run else { panic!() };
        let command = with.iter().find(|(k, _)| k.as_str() == "command").map(|(_, v)| v.to_string()).unwrap();
        // A literal path, nothing a shell expands: no variable can turn it into `rm -rf /`.
        assert!(command.starts_with("sudo sh -c 'rm -rf /opt/isoloom && "), "{command}");
        assert!(!command.contains('$') && !command.contains('*') && !command.contains('`'), "{command}");
        assert_eq!(steps::PROJECT_DIR, "/opt/isoloom");
    }

    #[test]
    fn the_setup_script_waits_for_cloud_init_then_installs_docker() {
        let env = crate::testing::environment(
            "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, app: { image: nginx } }\n",
            isoloom_spec::model::Target::Docker,
        );
        let script = setup_script(&env);
        assert!(script.starts_with("#!/bin/sh\n# Generated by isoloom"), "{script}");
        // A host whose login is root (Linode's) runs it without sudo: the script says neither.
        assert!(script.contains("piped into `sh -s` as root") && !script.contains("sudo"), "{script}");
        assert!(
            script.ends_with(&format!(
                "set -e\ncloud-init status --wait >/dev/null 2>&1 || true\n{}\n{PUBLISHED_FROM_OUTSIDE}",
                install_docker()
            )),
            "{script}"
        );
    }

    #[test]
    fn accept_env_is_none_without_forwarded_vars_and_lists_them_otherwise() {
        assert_eq!(accept_env(&guest(Vec::new()), AsRoot::Sudo).unwrap(), None);
        let cmd = accept_env(
            &guest(vec![slot("TOKEN", "LAUNCH_TOKEN").unwrap(), slot("DEMO_SECRET", "DEMO_SECRET").unwrap()]),
            AsRoot::Sudo,
        )
        .unwrap()
        .expect("a command when a value is forwarded");
        let line = cmd.command.to_string();
        assert!(line.contains("AcceptEnv DEMO_SECRET LAUNCH_TOKEN"), "{line}");
        assert!(line.contains("/etc/ssh/sshd_config.d/isoloom-env.conf"), "{line}");
        // Render's one command, as root: the drop-in directory is created first, since a bare
        // Terraform-made VM may lack it and the forwarded launch values would arrive unset.
        assert!(line.starts_with("sudo sh -c 'mkdir -p /etc/ssh/sshd_config.d &&"), "{line}");
        assert!(line.contains("systemctl reload ssh || systemctl reload sshd"), "{line}");
    }
}
