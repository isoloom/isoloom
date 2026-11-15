//! A topology editor's engine, compiled to WebAssembly: a topology (the JSON of an
//! [`isoloom_topology::Topology`] a caller hands it) becomes an `isoloom.yml` in Isoloom's
//! format, the files every Isoloom target generates from it, and one zip to download. Nothing
//! leaves the browser: Isoloom itself produces the Docker, Vagrant and Terraform files. The same
//! build runs in a server's JavaScript runtime too.
//!
//! The functions JavaScript calls are the `#[wasm_bindgen]` ones ([`plan_json`], [`bundle`],
//! [`bundle_target`], [`own_files_json`], and [`choices_json`], Isoloom's own lists, which a
//! caller's OS, appliance and protocol fields offer). The JSON they hand out is a [`Plan`],
//! [`File`]s and [`Choices`], which a Rust caller (a server, a test) reads back into those types. They are for `wasm32-unknown-unknown`: their errors are JavaScript
//! `Error`s, which exist only there, so built for another target an export that fails panics.
//!
//! ```
//! let topology = r#"{
//!   "name": "pair",
//!   "networks": { "office": { "cidr": "10.10.1.0/24" } },
//!   "nodes": {
//!     "web":  { "kind": { "container": { "image": "nginx:1.27-alpine" } }, "networks": { "office": null } },
//!     "user": { "kind": { "vm": { "os": "debian@12" } }, "access": true, "networks": { "office": null } }
//!   }
//! }"#;
//! let plan: isoloom_wasm::Plan = serde_json::from_str(&isoloom_wasm::plan_json(topology).unwrap())?;
//! assert!(plan.ok && plan.problems.is_empty(), "{:?}", plan.problems);
//! assert!(plan.files.iter().any(|f| f.path == ".isoloom/docker/compose.yml"));
//! assert_eq!(plan.addresses["web"], [("office".to_owned(), "10.10.1.10".to_owned())]);
//! let choices: isoloom_wasm::Choices = serde_json::from_str(&isoloom_wasm::choices_json().unwrap())?;
//! assert!(choices.os.iter().any(|os| os == "debian@12"));
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! The modules: `plan` (what a caller shows), `bundle` (the zips to download), `files` (the
//! files handed out, the project's own among them), `choices` (Isoloom's lists), `js` (the JavaScript
//! functions) and `error`.

mod bundle;
mod choices;
mod error;
mod files;
mod js;
mod plan;

#[cfg(test)]
mod tests;

pub(crate) use bundle::{bundle_bytes, bundle_target_bytes};
pub use choices::Choices;
pub(crate) use choices::choices;
pub(crate) use error::{Error, Result};
pub use files::File;
pub(crate) use files::own_files;
pub use js::{bundle, bundle_target, choices_json, own_files_json, plan_json};
pub use plan::Plan;
pub(crate) use plan::plan;
