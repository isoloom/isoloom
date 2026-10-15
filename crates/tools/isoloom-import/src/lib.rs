//! Drafting an Isoloom spec from files people already have: a Compose file ([`compose`]), the
//! `.tf` files of a Terraform folder ([`terraform`]), or what a Vagrantfile set when it ran
//! ([`vagrant`]). Each draft is written in the spec's format, ready for `isoloom validate`; it
//! is a starting point, and every import also returns a [`Note`] for each thing the source said
//! that the draft doesn't carry as it was.
//!
//! It sits with the tools, on the core alone (`isoloom-spec` for the format's rules), and is
//! pure: the command line reads the files (and runs the Vagrantfile's recorder) and hands this
//! crate their text.
//!
//! # Example
//!
//! ```
//! use isoloom_import::{NoteKind, compose};
//!
//! let draft = compose::draft(
//!     "services:\n  web:\n    image: nginx:1.27\n    ports: [\"8080:80\"]\n    command: nginx -g 'daemon off;'\n",
//!     "shop",
//!     "compose.yaml",
//! )?;
//! assert!(draft.yaml.contains("name: shop"));
//! assert!(draft.yaml.contains("ports: { http: { port: 80, publish: 8080 } }"));
//! assert!(draft.notes.iter().any(|n| n.at == "services.web.command" && n.kind == NoteKind::InImage));
//! # Ok::<(), isoloom_import::Error>(())
//! ```

pub mod compose;
mod draft;
mod error;
mod naming;
mod quantity;
pub mod terraform;
pub mod vagrant;

pub use draft::{Draft, Note, NoteKind};
pub use error::{Error, Result};
