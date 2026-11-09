//! The HCL builder: a Terraform module is built as `hcl-rs` structures ([`Block`],
//! attributes, [`Expression`]) and written once.
//!
//! - Values go in as values: a string is an [`Expression::String`], escaped by `hcl-rs` (quotes,
//!   backslashes, newlines, and Terraform's `${` and `%{` markers). Text that holds Terraform
//!   expressions is a [`Template`]: its text is escaped the same way and its expressions are
//!   interpolated, so nothing in a value can end the string or start an interpolation.
//! - [`Module`] writes the blocks in the layout `terraform fmt` keeps: two-space indents, the
//!   `=` of consecutive one-line attributes aligned, one blank line between top-level blocks. A
//!   list, an object or a function call is written on one line when it fits in `WIDTH`
//!   characters, else one item a line (a call keeps its last argument hugged: `f(a, [`).
//! - Comments go above a top-level block ([`Module::commented`]): `hcl-rs` has none inside.

mod escape;
mod expr;
mod template;
mod write;

pub(crate) use expr::{attr, binary, boolean, call, conditional, entry, for_list, index, list, not, null, number, object, reference, string};
pub(crate) use hcl::expr::BinaryOperator;
pub(crate) use hcl::{Block, BlockBuilder, Expression};
pub(crate) use template::{Piece, Template};
pub(crate) use write::Module;

/// The literal a module holds for the string `text`.
pub(crate) fn quote(text: &str) -> String {
    format!("\"{}\"", escape::quoted_inside(text))
}

/// Why a module couldn't be written (`hcl-rs` documents formatting as infallible: kept typed
/// rather than assumed).
pub(crate) type Error = hcl::Error;

#[cfg(test)]
mod tests;
