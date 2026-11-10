//! The model's address blocks: IPv4 ([`Cidr`]) and IPv6 ([`Cidr6`]), each parsed once into a
//! type that holds only a valid block, the addresses Isoloom reserves in one, and where each
//! network lands on the Docker target ([`docker_cidrs`]).

mod cidr;
mod cidr6;
mod docker;

pub use cidr::{Cidr, PRIVATE};
pub use cidr6::{Cidr6, PREFIX as IPV6_PREFIX};
pub(crate) use docker::DOCKER_BLOCK;
pub use docker::docker_cidrs;
