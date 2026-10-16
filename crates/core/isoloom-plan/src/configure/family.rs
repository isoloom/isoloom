//! How an OS installs and starts things: its family. The catalogue owns it (its daemon table
//! names units and packages per family); the configuration model resolves a recipe for the family
//! of its machine's OS.

pub use isoloom_catalogue::{Family, codename};
