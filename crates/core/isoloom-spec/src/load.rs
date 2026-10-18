//! Reading a spec: from its text, or from its project folder (the spec file found and read,
//! merged with a profile and `-s` overrides, then parsed).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::{defaults, profile, syntax};

/// The spec file names, at the root of a project: `isoloom.yml` (canonical) or `isoloom.yaml`.
pub const SPEC_FILES: &[&str] = &[SPEC_FILE, "isoloom.yaml"];

/// The spec file's usual name, the one a spec read from text is reported under.
pub const SPEC_FILE: &str = "isoloom.yml";

/// Reads a spec from YAML text, written in the format authors write ([`syntax`]).
///
/// # Errors
///
/// [`Error::Invalid`] with every problem found, coded and placed (`file` names the spec file
/// in their first line).
pub fn parse(yaml: &str) -> Result<syntax::Spec> {
    parse_file(yaml, SPEC_FILE)
}

/// [`parse`], the problems naming the spec file `file`.
fn parse_file(yaml: &str, file: &str) -> Result<syntax::Spec> {
    syntax::parse(yaml).map_err(|problems| Error::Invalid {
        file: file.to_owned(),
        problems,
    })
}

/// The spec file in `dir`: `isoloom.yml` or `isoloom.yaml`, never both.
///
/// # Errors
///
/// [`Error::NotFound`] when `dir` has neither, [`Error::Ambiguous`] when it has both.
pub fn find(dir: &Path) -> Result<PathBuf> {
    let found: Vec<PathBuf> = SPEC_FILES.iter().map(|f| dir.join(f)).filter(|p| p.is_file()).collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Error::NotFound(dir.to_path_buf())),
        _ => Err(Error::Ambiguous(dir.to_path_buf())),
    }
}

/// Reads the spec in `dir` (see [`find`]).
///
/// # Errors
///
/// [`Error`]: no spec file (or two), one that can't be read, or its problems.
pub fn load(dir: &Path) -> Result<syntax::Spec> {
    load_with(dir, &[])
}

/// Reads the spec in `dir` with `-s key=value` overrides applied first (see
/// [`defaults::override_spec`]); keys prefixed `defaults.` are left to the defaults.
///
/// # Errors
///
/// [`Error`], as [`load`], or an override that doesn't apply.
pub fn load_with(dir: &Path, sets: &[String]) -> Result<syntax::Spec> {
    load_profiled(dir, None, sets)
}

/// Reads the spec in `dir` with a profile's `spec` merged in (checked against the spec first:
/// see [`profile::Profile::check`]), then `-s key=value` overrides.
///
/// # Errors
///
/// [`Error`], as [`load_with`], or a profile that names what the spec doesn't have.
pub fn load_profiled(dir: &Path, profile: Option<&profile::Profile>, sets: &[String]) -> Result<syntax::Spec> {
    let path = find(dir)?;
    let file = path.file_name().map_or_else(|| SPEC_FILE.to_owned(), |f| f.to_string_lossy().into_owned());
    let text = std::fs::read_to_string(&path).map_err(|source| Error::Read { path, source })?;
    let text = match profile {
        Some(p) => {
            let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
            let problems = p.check(&doc);
            if !problems.is_empty() {
                return Err(Error::ProfileMismatch { problems });
            }
            defaults::override_spec_with(&text, Some(&p.spec), sets)
        }
        None => defaults::override_spec(&text, sets),
    }?;
    parse_file(&text, &file)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const SPEC: &str = "version: 1\nname: shop\nnetworks: { lab: { cidr: 10.1.0.0/24 } }\nmachines:\n  web: { os: debian@12, networks: { lab: 10 } }\n";

    /// A fresh folder for test `name`, holding `files` (name, text).
    fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = crate::testing::folder(&format!("load-{name}")).unwrap();
        for (file, text) in files {
            std::fs::write(dir.join(file), text).unwrap();
        }
        dir
    }

    #[test]
    fn text_reads_into_the_spec_or_its_problems_under_the_spec_files_name() {
        assert_eq!(parse(SPEC).unwrap().name, "shop");
        match parse("version: 1\nname: shop\nmachines:\n  web: { docker: { image: x } }\n") {
            Err(Error::Invalid { file, problems }) => assert_eq!((file.as_str(), problems.len()), ("isoloom.yml", 1)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_folder_has_one_spec_file() {
        let none = folder("none", &[]);
        assert!(matches!(find(&none), Err(Error::NotFound(d)) if d == none));
        let yaml = folder("yaml", &[("isoloom.yaml", SPEC)]);
        assert_eq!(find(&yaml).unwrap(), yaml.join("isoloom.yaml"));
        let both = folder("both", &[("isoloom.yml", SPEC), ("isoloom.yaml", SPEC)]);
        assert!(matches!(find(&both), Err(Error::Ambiguous(d)) if d == both));
    }

    #[test]
    fn a_folder_reads_into_its_spec_and_overrides_apply_first() {
        let dir = folder("overrides", &[("isoloom.yml", SPEC)]);
        assert_eq!(load(&dir).unwrap().machines["web"].os.map(|os| os.to_string()), Some("debian@12".to_owned()));
        let set = load_with(&dir, &["machines.web.os=alpine".to_owned(), "defaults.cloud.aws.region=x".to_owned()]).unwrap();
        assert_eq!(set.machines["web"].os.map(|os| os.to_string()), Some("alpine@3.19".to_owned()));
        assert!(matches!(load_with(&dir, &["nonsense".to_owned()]), Err(Error::SetShape { .. })));
    }

    #[test]
    fn problems_name_the_file_they_are_in() {
        let dir = folder("named", &[("isoloom.yaml", "version: 1\nname: shop\nmachines:\n  web: { vm: {} }\n")]);
        match load(&dir) {
            Err(Error::Invalid { file, .. }) => assert_eq!(file, "isoloom.yaml"),
            other => panic!("{other:?}"),
        }
    }
}
