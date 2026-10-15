//! The frame of every Terraform module the crate writes: its first lines ([`Usage`]), the
//! `terraform` block and its providers ([`providers`]), its variables ([`variable`], the
//! launch-time [`inputs`] among them), its resources' identifiers, its outputs, and its file,
//! the project's lists at its end.

pub(crate) mod inputs;
pub(crate) mod providers;
mod usage;
pub(crate) mod variable;

use isoloom_render::hcl_builder::{Block, BlockBuilder, Expression, Module, boolean, number};
use isoloom_render::{GeneratedFile, OUTPUT_DIR, project};
use isoloom_spec::model::Target;

pub(crate) use usage::Usage;

use crate::{Error, Result};

/// A Terraform identifier from a machine, network, disk or OS name: `web-1` is `web_1`,
/// `ubuntu-24.04` is `ubuntu_24_04`.
pub(crate) fn ident(name: &str) -> String {
    name.replace(['-', '.'], "_")
}

/// A `resource` block, started.
pub(crate) fn resource(kind: &str, name: &str) -> BlockBuilder {
    Block::builder("resource").add_label(kind).add_label(name)
}

/// An `output` block.
pub(crate) fn output(name: &str, value: Expression) -> Block {
    Block::builder("output").add_label(name).add_attribute(("value", value)).build()
}

/// A generated password, `random_password.<name>`: 24 letters and digits, two of each kind at
/// least, so every OS's password rules take it.
pub(crate) fn password(name: &str) -> Block {
    resource("random_password", name)
        .add_attribute(("length", number(24)))
        .add_attribute(("special", boolean(false)))
        .add_attribute(("min_upper", number(2)))
        .add_attribute(("min_lower", number(2)))
        .add_attribute(("min_numeric", number(2)))
        .build()
}

/// How a module's machines get the project's files, which decides the lists it ends with
/// (`isoloom_render::project`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectCopy {
    /// An archive made where Terraform runs, with `tar`: its exclusion list is a file too.
    Archive,
    /// Each file written into the machine by its cloud-init (Proxmox).
    Files,
}

/// A module, started with its header and how to start and stop it.
pub(crate) fn start(usage: &Usage) -> Module {
    Module::new(usage.text())
}

/// Module `m` of `target` as its file, `.isoloom/<folder>/main.tf`, ending with the project's
/// lists.
pub(crate) fn file(target: Target, folder: &str, mut m: Module, copy: ProjectCopy) -> Result<GeneratedFile> {
    m.fragment(&project::lists(copy == ProjectCopy::Archive));
    let contents = m.render().map_err(|source| Error::Module {
        target,
        folder: folder.to_owned(),
        source,
    })?;
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{folder}/main.tf"),
        contents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_has_no_dash_or_dot() {
        assert_eq!(ident("web-1"), "web_1");
        assert_eq!(ident("ubuntu-24.04"), "ubuntu_24_04");
    }

    #[test]
    fn a_module_file_ends_with_the_project_lists_it_copies_with() {
        let usage = Usage::with_key("t", "");
        let archived = file(Target::CloudVm, "t", start(&usage), ProjectCopy::Archive).unwrap();
        assert_eq!(archived.path, ".isoloom/t/main.tf");
        assert!(archived.contents.contains("local_file\" \"isoloom_project\""), "{}", archived.contents);
        let written = file(Target::Proxmox, "t", start(&usage), ProjectCopy::Files).unwrap();
        assert!(!written.contents.contains("local_file"), "{}", written.contents);
    }
}
