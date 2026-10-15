//! The Vagrant backends: `vagrant` (one VM per machine, every provider), and the Vagrant sides of
//! `docker-vm` (Docker on one local VM) and `hybrid` (containers and VMs side by side), which
//! `isoloom-targets` puts together with their Compose files. Written with one Ruby builder
//! ([`ruby`]) and the Vagrantfile pieces they share ([`vagrantfile`]).

pub mod docker_vm;
pub mod hybrid;
pub mod ruby;
pub mod vagrant;
pub mod vagrantfile;

use isoloom_render::router;
