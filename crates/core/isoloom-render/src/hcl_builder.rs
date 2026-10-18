//! The HCL builder: a Terraform module is built as `hcl-rs` structures ([`Block`],
//! [`Attribute`], [`Expression`]) and written once.
//!
//! - Values go in as values: a string is an [`Expression::String`], escaped by `hcl-rs` (quotes,
//!   backslashes, newlines, and Terraform's `${` and `%{` markers). Text that holds Terraform
//!   expressions is a [`Template`]: its text is escaped the same way and its expressions are
//!   interpolated, so nothing in a value can end the string or start an interpolation.
//! - [`Module`] writes the blocks in the layout `terraform fmt` keeps: two-space indents, the
//!   `=` of consecutive one-line attributes aligned, one blank line between top-level blocks. A
//!   list, an object or a function call is written on one line when it fits in [`WIDTH`]
//!   characters, else one item a line (a call keeps its last argument hugged: `f(a, [`).
//! - Comments go above a top-level block ([`Module::commented`]): `hcl-rs` has none inside.

mod escape;
mod expr;
mod template;
mod write;

pub use expr::{attr, binary, boolean, call, conditional, entry, for_list, index, list, not, null, number, object, reference, string};
pub use hcl::expr::BinaryOperator;
pub use hcl::{Attribute, Block, BlockBuilder, Body, Expression, Structure};
pub use template::{Piece, Template};
pub use write::{Module, WIDTH};

/// Why a module couldn't be written (`hcl-rs` documents formatting as infallible: kept typed
/// rather than assumed).
pub type Error = hcl::Error;
