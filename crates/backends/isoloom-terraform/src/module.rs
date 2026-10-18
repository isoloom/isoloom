//! The frame of every Terraform module the crate writes: its first lines ([`Usage`]), the
//! `terraform` block and its providers ([`providers`]), its variables ([`variable`], the
//! launch-time [`inputs`] among them), its resources' identifiers, its outputs, and its file,
//! the project's lists at its end.

pub(crate) mod inputs;
pub(crate) mod manifest;
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
/// (`isoloom_render::project`), under the name the modules use.
pub(crate) use isoloom_render::project::Copy as ProjectCopy;

/// A module, started with its header and how to start and stop it.
pub(crate) fn start(usage: &Usage) -> Module {
    Module::new(usage.text())
}

/// Module `m` of `target` as its file, `.isoloom/<folder>/main.tf`, ending with the project's
/// lists it reads.
pub(crate) fn file(target: Target, folder: &str, mut m: Module, copy: ProjectCopy) -> Result<GeneratedFile> {
    let failed = |source| Error::Module {
        target,
        folder: folder.to_owned(),
        source,
    };
    let lists = project::lists(copy, &m.render().map_err(failed)?);
    if !lists.is_empty() {
        m.fragment(&lists);
    }
    Ok(GeneratedFile {
        path: format!("{OUTPUT_DIR}/{folder}/main.tf"),
        contents: m.render().map_err(failed)?,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn an_identifier_has_no_dash_or_dot() {
        assert_eq!(ident("web-1"), "web_1");
        assert_eq!(ident("ubuntu-24.04"), "ubuntu_24_04");
    }

    proptest! {
        #[test]
        fn a_name_made_an_identifier_keeps_its_length_and_stays_one(name in "[a-z0-9._-]{1,24}") {
            let id = ident(&name);
            prop_assert!(!id.contains(['-', '.']), "{id}");
            prop_assert_eq!(id.len(), name.len());
            prop_assert_eq!(ident(&id), id);
        }
    }

    #[test]
    fn a_module_file_ends_with_the_project_lists_it_copies_with() {
        let usage = Usage::with_key("t", "");
        let archived = file(Target::CloudVm, "t", start(&usage), ProjectCopy::Archive).unwrap();
        assert_eq!(archived.path, ".isoloom/t/main.tf");
        assert!(archived.contents.contains("local_file\" \"isoloom_project\""), "{}", archived.contents);
        assert!(!archived.contents.contains("project_paths"), "{}", archived.contents);
        let mut reading = start(&usage);
        reading.block(output("files", isoloom_render::hcl_builder::reference("local.project_paths")));
        let written = file(Target::Proxmox, "t", reading, ProjectCopy::Files).unwrap();
        assert!(written.contents.contains("  project_paths = [for f"), "{}", written.contents);
        assert!(
            !written.contents.contains("local_file") && !written.contents.contains("project_ignored"),
            "{}",
            written.contents
        );
        let unread = file(Target::Proxmox, "t", start(&usage), ProjectCopy::Files).unwrap();
        assert!(!unread.contents.contains("project_"), "{}", unread.contents);
    }
}
