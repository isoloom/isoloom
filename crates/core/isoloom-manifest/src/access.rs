//! The on-demand access machine (a machine a user works from, started when asked) contract, shared by the backends
//! that render it and the runner that hot-adds it. An access machine is marked [`crate::Machine::on_demand`]
//! (never brought up by the initial `up`); the runner later brings it up onto a chosen network of the environment.
//!
//! - Compose: the access service sits in the [`PROFILE`] Compose profile, so `up` skips it; the
//!   runner hot-adds it with `docker compose --profile access up -d --no-deps <service>`, setting
//!   [`contract::ACCESS_IMAGE`](crate::contract::ACCESS_IMAGE) to the image the runner was given
//!   (its default is baked into the service).
//! - Vagrant: the access VM is defined `autostart: false`, so `vagrant up` skips it; the runner
//!   hot-adds it with `vagrant up <name> --provider=<p>`, setting
//!   [`contract::ACCESS_BOX`](crate::contract::ACCESS_BOX) to the box it was given, and takes it
//!   out with `vagrant destroy -f <name>`.

/// The Compose profile the access service sits in (so the initial `up` skips it).
pub const PROFILE: &str = "access";
