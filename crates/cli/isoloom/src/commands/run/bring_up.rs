//! The command (and its working directory) that brings each target up or tears it down.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use isoloom_spec::Target;

use crate::tools::compose;
use crate::{Res, target};

/// Which way a target goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// Up.
    Up,
    /// Down, the volumes kept past `down` (`persistent`) left in place.
    Down,
    /// Down, every volume removed (`down --volumes`).
    DownWithVolumes,
}

/// The command (and its working directory) that brings a target up or tears it down.
pub fn bring_up(dir: &Path, t: Target, cloud: Option<&str>, instance: Option<u8>, way: Way) -> Res<Invocation> {
    let out = dir.join(isoloom_spec::instance::output_dir(instance));
    let down = way != Way::Up;
    Ok(match t {
        Target::Docker | Target::Hosted => compose_up(dir, &out, instance, way),
        Target::Vagrant | Target::DockerVm | Target::Hybrid => {
            let args = if down { vec![s("destroy"), s("-f")] } else { vec![s("up")] };
            invocation("vagrant", args, &out.join(target::vm_folder(t)))
        }
        Target::Kubernetes => {
            // kustomize build, piped to kubectl; a shell keeps it one command.
            let kdir = out.join("kubernetes");
            let build = format!("kubectl kustomize --load-restrictor LoadRestrictionsNone {}", kdir.display());
            // Down leaves what is labelled to be kept (persistent claims, their namespace).
            let verb = match way {
                Way::Up => "apply",
                Way::Down => "delete --ignore-not-found=true -l 'isoloom.persistent!=true'",
                Way::DownWithVolumes => "delete --ignore-not-found=true",
            };
            invocation("sh", vec![s("-c"), format!("{build} | kubectl {verb} -f -")], dir)
        }
        Target::Proxmox | Target::Esxi => tf_run(down, out.join(t.id())),
        Target::CloudVm => tf_run(down, cloud_vm_module(&out, cloud)?),
        Target::CloudDocker => tf_run(down, out.join("cloud-docker").join(target::docker_cloud(cloud)?)),
        // Nothing to create or destroy: `run` provisions over SSH (see external::external_up),
        // `down` only forgets the environment.
        Target::External => invocation("true", vec![], dir),
        // The generated scripts run Terraform on the project's module (see generate::cloud_services).
        Target::CloudServices => invocation("sh", vec![s(if down { "down.sh" } else { "up.sh" })], &out.join("cloud-services")),
    })
}

fn s(x: &str) -> String {
    x.to_string()
}

fn invocation(program: &str, args: Vec<String>, wd: &Path) -> Invocation {
    Invocation {
        program: s(program),
        args,
        wd: wd.to_path_buf(),
    }
}

/// Docker Compose up or down, under the project name it runs as (a tool's own, from the
/// registry), so `down` finds it.
fn compose_up(dir: &Path, out: &Path, instance: Option<u8>, way: Way) -> Invocation {
    // Down with the `check` profile too, so the runners' stand-ins go as well.
    let mut args = vec![s("compose")];
    args.extend(compose::compose_files(dir, instance));
    let yaml = std::fs::read_to_string(out.join("docker/compose.yml")).unwrap_or_default();
    if way != Way::Up {
        args.extend([s("--profile"), s("check")]);
        let project = compose::compose_project(dir, instance);
        if let Some(removable) = isoloom_targets::removable_volumes(&yaml, project.as_deref()).filter(|_| way == Way::Down) {
            return keeping_volumes(dir, &args, &removable);
        }
        args.extend([s("down"), s("-v")]);
        return invocation("docker", args, dir);
    }
    // One-shot jobs nothing running waits for fail `up --wait`: wait for the rest,
    // then run them attached, in order (see isoloom_targets::start_plan).
    let plan = isoloom_targets::start_plan(&yaml).unwrap_or_default();
    if plan.jobs.is_empty() {
        args.extend([s("up"), s("-d"), s("--build"), s("--wait")]);
        return invocation("docker", args, dir);
    }
    let q = isoloom_spec::shell::quote;
    let prefix = std::iter::once("docker".to_string())
        .chain(args.iter().map(|a| q(a)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut script = format!("{prefix} up -d --build --wait {}", plan.wait.iter().map(|w| q(w)).collect::<Vec<_>>().join(" "));
    for j in &plan.jobs {
        let _ = write!(script, " && {prefix} up --no-deps --exit-code-from {j} {j}", j = q(j));
    }
    invocation("sh", vec![s("-c"), script], dir)
}

/// Down, the volumes kept past it left: the containers and their anonymous volumes removed, then
/// the networks, then the named volumes that aren't kept (`removable`, by their Docker names).
fn keeping_volumes(dir: &Path, compose: &[String], removable: &[String]) -> Invocation {
    let q = isoloom_spec::shell::quote;
    let prefix = std::iter::once("docker".to_string())
        .chain(compose.iter().map(|a| q(a)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut script = format!("{prefix} rm -f -s -v && {prefix} down");
    if !removable.is_empty() {
        let names = removable.iter().map(|n| q(n)).collect::<Vec<_>>().join(" ");
        let _ = write!(script, " && docker volume rm -f {names}");
    }
    invocation("sh", vec![s("-c"), script], dir)
}

/// The cloud-vm module of the cloud picked (`target::vm_cloud`; AWS for an entry recorded
/// before the cloud was). Each cloud's module is written only when the cloud can run the
/// environment, so a refused cloud has no directory.
fn cloud_vm_module(out: &Path, cloud: Option<&str>) -> Res<PathBuf> {
    let cloud = cloud.unwrap_or("aws");
    let dir = out.join("cloud-vm").join(cloud);
    if !dir.exists() {
        return Err(format!(
            "`cloud-vm` has no `{cloud}` module for this lab (either the cloud is unknown or its driver can't run this spec); run `isoloom generate` and check .isoloom/cloud-vm/"
        )
        .into());
    }
    Ok(dir)
}

/// The command that brings a target up or tears it down, and the folder it runs in.
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub wd: PathBuf,
}

/// Terraform brings a module up with init then apply, and down with destroy. `run` shells out
/// once, so init+apply go through a short shell; the cloud's own credentials come from the
/// environment, as Terraform expects.
fn tf_run(down: bool, module: PathBuf) -> Invocation {
    let script = if down {
        "terraform init -input=false && terraform destroy -auto-approve".to_string()
    } else {
        "terraform init -input=false && terraform apply -auto-approve".to_string()
    };
    Invocation {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        wd: module,
    }
}
