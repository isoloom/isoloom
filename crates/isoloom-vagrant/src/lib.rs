//! The Vagrant backends: `vagrant` (one VM per machine, every provider), `docker-vm` (Docker on
//! one local VM) and `hybrid` (containers and VMs side by side), written with one Ruby builder
//! ([`ruby`]) and the Vagrantfile pieces they share ([`vagrantfile`]).

pub mod docker_vm;
pub mod hybrid;
pub mod ruby;
pub mod vagrant;
pub mod vagrantfile;

pub use docker_vm::DockerVmBackend;
pub use hybrid::HybridBackend;

use isoloom_compose::docker;
use isoloom_plan::checks;
use isoloom_render::{
    CONTROLLER_BOX, GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, constraints, controller_box, controller_size, header,
    host_vars, machine_setup, names_of, netmask, router, start_order, trunks,
};
use isoloom_spec::{groups, images, model, validate};
