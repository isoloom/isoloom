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
use isoloom_render::router;
