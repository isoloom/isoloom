//! The platform level (`services:`, the `paas` feature): services an environment uses beside its
//! machines, each declared by what it is (a Postgres database, an object-storage bucket), never by
//! a provider's resource. Every target renders a service its own way or refuses it with a reason:
//! a cloud as its managed service, every other target as a machine running the self-hosted
//! equivalent. Nothing is passed through.
//!
//! A service is reached by its name in the environment's private zone (`<service>.<domain>`: a
//! spec with services has `dns.private` on, whatever it says), and its clients get its facts
//! through what machines already have: `environment` values, and files (its credentials, and
//! what differs per platform), which a cloud delivers once it has made the service.

mod check;
mod model;
mod placement;

pub use check::check;
pub use model::{Access, Api, Client, Credentials, Kafka, KafkaCredentials, ObjectStorage, Postgres, Publish, Service, Size, Topic};
pub use placement::{Placement, placement};

#[cfg(test)]
mod tests;
