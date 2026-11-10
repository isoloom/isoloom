//! The Docker host VM, as both Vagrantfiles that run Compose in a VM write it (`docker-vm`'s VM,
//! hybrid's containers' host): its set-up, in order, as named provisioners (which `vagrant up
//! --provision-with` takes), and the provisioner that starts its Compose file.

use isoloom_plan::{Environment, Step};
use isoloom_render::steps;

use crate::ruby::{Provision, Ruby, Value};
use crate::vagrantfile::{self, shell};
use crate::vms::values;

/// The project provisioner.
pub(crate) const PROJECT: &str = "project";

/// The provisioner trusting the spec's certificate authorities, from the project's copy.
pub(crate) const CA_CERTIFICATES: &str = "ca-certificates";

/// The VM `var`'s set-up, before anything of the environment runs there: the system constraints
/// when the spec has some; the project, with `outputs` of Isoloom's (first, so the certificate
/// authorities it holds are trusted before Docker downloads); those authorities when the spec
/// names some; then Docker, with published ports reaching every container
/// ([`steps::PUBLISHED_FROM_OUTSIDE`]).
pub(crate) fn set_up(r: &mut Ruby, env: &Environment, var: &str, outputs: &[&str]) {
    if let Some(sh) = steps::linux(env, "", &Step::Prelude) {
        r.provision(Provision::named(var, vagrantfile::CONSTRAINTS, "shell").kw("inline", Value::str(&sh)));
    }
    r.provision(Provision::named(var, PROJECT, "isoloom_project").kw("rules", crate::vms::copy_rules(outputs)));
    if let Some(sh) = steps::trust_authorities(env) {
        r.provision(Provision::named(var, CA_CERTIFICATES, "shell").kw("inline", Value::str(&sh)));
    }
    let docker = format!("{}\n{}", steps::install_docker(), steps::PUBLISHED_FROM_OUTSIDE);
    r.provision(Provision::named(var, vagrantfile::DOCKER, "shell").kw("inline", Value::str(&docker)));
}

/// The provisioner `name` of VM `var` that starts its Compose file: the values given at launch
/// decoded (they come in base64), the containers' secrets read once delivered, then `start`.
pub(crate) fn start(r: &mut Ruby, env: &Environment, var: &str, name: &str, start: &str) {
    let inputs = &env.input_names();
    let secrets = isoloom_render::secrets::host_environment(env, &isoloom_render::secrets::of_containers(env));
    let script = format!("{}{secrets}{start}", values::linux_decode(inputs));
    let encoded = (!inputs.is_empty()).then(|| values::encoded(inputs));
    r.provision(shell(var, name).kw("inline", Value::str(&script)).kw_opt("env", encoded));
}
