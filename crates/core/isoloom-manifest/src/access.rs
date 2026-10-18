//! The on-demand access machine (the learner's attacker box) contract, shared by the backends
//! that render it and the runner that hot-adds it. An access machine is marked [`crate::Machine::on_demand`]
//! (never brought up by the initial `up`); the runner later brings it up onto a chosen lab network.
//!
//! - Compose: the access service sits in the [`PROFILE`] Compose profile, so `up` skips it; the
//!   runner hot-adds it with `docker compose --profile access up -d --no-deps <service>`, setting
//!   [`IMAGE_ENV`] to the image the learner picked (its default is baked into the service).
//! - Vagrant: the access VM is defined `autostart: false`, so `vagrant up` skips it; the runner
//!   hot-adds it with `vagrant up <name>`, setting [`BOX_ENV`] to the box the learner picked.

/// The Compose profile the access service sits in (so the initial `up` skips it).
pub const PROFILE: &str = "access";

/// The environment variable the access Compose service reads its image from (its default, the
/// resolved access image, is the Compose `${VAR:-default}` fallback). The runner sets it to the
/// learner's picked image when it hot-adds the box.
pub const IMAGE_ENV: &str = "ISOLOOM_ACCESS_IMAGE";

/// The environment variable the access Vagrant box reads its box name from (its default is the
/// `ENV.fetch` fallback). The runner sets it to the learner's picked box when it hot-adds.
pub const BOX_ENV: &str = "ISOLOOM_ACCESS_BOX";

/// The image an access Compose service runs when the learner picks nothing of their own.
pub const DEFAULT_IMAGE: &str = "kalilinux/kali-rolling";

/// The Vagrant box an access VM runs when the learner picks nothing of their own.
pub const DEFAULT_BOX: &str = "kalilinux/rolling";
