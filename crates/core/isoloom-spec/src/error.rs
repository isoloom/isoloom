//! Why a spec, a file beside it or a value in it isn't one Isoloom reads: one enum for the
//! crate. Every message says where (the folder, the file, the field, the value) and why, in
//! words a user acts on without reading Isoloom's code. A spec that reads but is wrong is
//! [`Error::Invalid`], holding its [`Problem`]s, one per field.

use std::fmt;
use std::path::PathBuf;

use crate::model::Target;
use crate::profile::ProfileProblem;
use crate::syntax::Problem;

/// Why a spec, a file beside it or a value in it isn't one Isoloom reads.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The folder has no spec file.
    #[error("no isoloom.yml in {}", .0.display())]
    NotFound(PathBuf),
    /// Both `isoloom.yml` and `isoloom.yaml` exist: which one is meant is ambiguous.
    #[error("both isoloom.yml and isoloom.yaml exist in {}; keep one", .0.display())]
    Ambiguous(PathBuf),
    /// A file exists but couldn't be read.
    #[error("can't read {}: {source}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// The system's reason.
        #[source]
        source: std::io::Error,
    },
    /// Not YAML: the spec's text before its overrides, or a profile's spec it merges. The
    /// message carries the line and column.
    #[error("{0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    /// The spec's problems: every one found, coded and placed, in file order.
    #[error("{}", Rendered { file, problems })]
    Invalid {
        /// The spec file the problems are placed in (`isoloom.yml`).
        file: String,
        /// The problems.
        problems: Vec<Problem>,
    },
    /// A `-s` that isn't `key=value`.
    #[error("`-s {set}`: expected key=value, like -s machines.web.vm.os=ubuntu@24.04")]
    SetShape {
        /// The `-s` as given.
        set: String,
    },
    /// A `-s` whose key isn't a dotted path.
    #[error("`-s {set}`: the key is a dotted path, like cloud.aws.region")]
    SetKey {
        /// The `-s` as given.
        set: String,
    },
    /// A name that breaks its rule.
    #[error("`{value}` isn't a valid {what}: {rule}")]
    Name {
        /// What it names (`port name`).
        what: &'static str,
        /// The value written.
        value: String,
        /// The rule, as an author fixes the value.
        rule: &'static str,
    },
    /// An OS no family names.
    #[error("unknown OS `{text}`; write `<family>@<version>` with one of: {known}")]
    UnknownOs {
        /// What the spec wrote.
        text: String,
        /// The families and their versions.
        known: String,
    },
    /// A known OS family without that version.
    #[error("{family} has no version `{version}`; use one of: {known} (`{family}` alone is {family}@{default})")]
    UnknownOsVersion {
        /// The family.
        family: &'static str,
        /// The version the spec wrote.
        version: String,
        /// The family's versions.
        known: String,
        /// The family's default version.
        default: &'static str,
    },
    /// A file mode that isn't octal digits up to `0777`.
    #[error("`{0}` isn't a file mode: octal digits up to 0777, like \"0444\"")]
    Mode(String),
    /// A machine with both a catalogue `service` and a custom `app`.
    #[error("`service` and `app` are both given: a machine is a catalogue service or a custom app")]
    ServiceAndApp,
    /// A machine's `with` without a `service`.
    #[error("`with` sets a catalogue service's settings: it goes with `service`")]
    WithWithoutService,
    /// An app's `config` without an `appliance`.
    #[error("`config` is an appliance's: it goes with `appliance`")]
    ConfigWithoutAppliance,
    /// An app with both an `image` and a `build`.
    #[error("`image` and `build` are both given: a container comes from one")]
    ImageAndBuild,
    /// An appliance built from a Dockerfile.
    #[error("an appliance comes from its `image` (or `firmware`), not a `build`")]
    ApplianceBuild,
    /// `firmware` without `appliance: cisco-dynamips`.
    #[error("`firmware` is `cisco-dynamips`'s: it goes with `appliance: cisco-dynamips`")]
    FirmwareWithoutDynamips,
    /// An app with no recipe at all.
    #[error("an app needs a recipe: `image`, `build`, `steps` or `appliance`")]
    NoRecipe,
    /// `cisco-dynamips` with an `image`.
    #[error("`cisco-dynamips` boots its `firmware`; Isoloom builds its image, so no `image`")]
    DynamipsImage,
    /// `cisco-dynamips` without its `firmware`.
    #[error("`cisco-dynamips` needs the `firmware` it boots (the IOS `.bin`)")]
    DynamipsFirmware,
    /// Another appliance with a `firmware`.
    #[error("`firmware` is `cisco-dynamips`'s; this appliance runs its `image`")]
    ApplianceFirmware,
    /// An appliance without its `image`.
    #[error("an appliance runs its `image` (`vrnetlab/cisco_iol:<version>`)")]
    ApplianceImage,
    /// A step's `creates` without its `run`.
    #[error("`creates` goes with `run`")]
    CreatesWithoutRun,
    /// A step that does nothing.
    #[error("a step needs one of `run`, `copy`, `packages` and `playbook`")]
    NoStep,
    /// A step that does several things.
    #[error("a step does one thing: one of `run`, `copy`, `packages` and `playbook` per step")]
    SeveralSteps,
    /// A healthcheck with both a `test` and an `http` page.
    #[error("`test` and `http` are both given: a healthcheck is a `test` or an `http` page")]
    TestAndHttp,
    /// A healthcheck that tries nothing.
    #[error("a healthcheck says what to try: `{{ test: <command> }}` or `{{ http: {{ port, path }} }}`")]
    NoHealthProbe,
    /// A file with no source.
    #[error("a file needs one of `source`, `content`, `input` and `secret`")]
    NoFileSource,
    /// A file with several sources.
    #[error("`{}` are {} given: keep one source", .0.join("` and `"), if .0.len() == 2 { "both" } else { "all" })]
    FileSources(Vec<&'static str>),
    /// An environment value written as a reference to neither one input nor one secret.
    #[error("write one of `{{ input: NAME }}`")]
    EnvReference,
    /// A GPU count that is neither a whole number nor `all`.
    #[error("`gpus.count`: a whole number or `all`, not {0}")]
    GpuCount(String),
    /// `gpus` as a word other than `all`.
    #[error("`gpus`: a number, `all` or a mapping, not `{0}`")]
    GpusWord(String),
    /// A device written with more than three parts.
    #[error("`{0}`: a device is `host[:guest[:permissions]]`")]
    Device(String),
    /// A layer of the defaults (the user's or the project's file, `--images`) that isn't YAML.
    #[error("{}: {source}", path.display())]
    Layer {
        /// The file.
        path: PathBuf,
        /// What the YAML reader reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// The defaults, merged, hold a key or a value they don't have.
    #[error("defaults ({sources}): {source}")]
    Defaults {
        /// The layers merged, as `isoloom defaults` names them.
        sources: String,
        /// What the typed read reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// An image table that isn't one (`--images`, the defaults' `images:`).
    #[error("{0}")]
    Images(#[source] serde_yaml_ng::Error),
    /// A profile's name that isn't kebab-case.
    #[error("profile `{0}`: a profile name is kebab-case (lowercase letters, digits, dashes)")]
    ProfileName(String),
    /// No profile by that name.
    #[error("no profile `{name}` ({})", Known { folder, known })]
    NoProfile {
        /// The name asked for.
        name: String,
        /// The project's profiles folder.
        folder: PathBuf,
        /// The profiles it has, by name.
        known: Vec<String>,
    },
    /// Both `profiles/<name>.yml` and `.yaml` exist.
    #[error("both {}/{name}.yml and {}/{name}.yaml exist; keep one", crate::profile::DIR, crate::profile::DIR)]
    TwoProfiles {
        /// The profile's name.
        name: String,
    },
    /// A profile's file that isn't YAML, or holds a field a profile doesn't have.
    #[error("profile `{name}` ({}): {source}", path.display())]
    ProfileYaml {
        /// The profile's name.
        name: String,
        /// Its file.
        path: PathBuf,
        /// What the YAML reader reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// A profile's `spec` or `defaults` that isn't a mapping.
    #[error("profile `{name}` ({}): `{key}` is a mapping", path.display())]
    ProfileMapping {
        /// The profile's name.
        name: String,
        /// Its file.
        path: PathBuf,
        /// The field.
        key: &'static str,
    },
    /// A profile's input that isn't one value.
    #[error("profile `{name}` ({}): input `{input}` takes a single value (text, a number or a boolean)", path.display())]
    ProfileInput {
        /// The profile's name.
        name: String,
        /// Its file.
        path: PathBuf,
        /// The input.
        input: String,
    },
    /// The profile names what the spec doesn't have: one line per problem.
    #[error("{}", Lines(problems))]
    ProfileMismatch {
        /// The problems, one per field.
        problems: Vec<ProfileProblem>,
    },
    /// Another target than the one the profile runs on.
    #[error("profile `{profile}` runs on `{}`, not `{asked}`: drop the target or choose another profile", target.id())]
    ProfileTarget {
        /// The profile's name.
        profile: String,
        /// The target it runs on.
        target: Target,
        /// The target asked for.
        asked: String,
    },
    /// An instance number outside 1 to [`crate::instance::MAX`].
    #[error("an instance number is 1 to {}", crate::instance::MAX)]
    InstanceNumber(u8),
    /// An environment's name too long once it carries its instance.
    #[error("`{0}` is too long for an instance name (at most 55 characters with the suffix)")]
    InstanceName(String),
    /// Text that isn't an IPv4 block.
    #[error("`{0}` isn't an IPv4 block: write the network's own address, `/` and the prefix length (`10.20.0.0/24`)")]
    Cidr(String),
    /// Text that isn't an IPv6 /64 block.
    #[error("`{0}` isn't an IPv6 block: write a unique local or global network address and `/64` (`fd00:20::/64`)")]
    Cidr6(String),
    /// A published port too high to shift for the instance.
    #[error("machines.{machine}.ports: published port {port} leaves no room for instance {instance} (ports shift by 100 per instance)")]
    InstancePort {
        /// The machine.
        machine: String,
        /// The published port.
        port: u16,
        /// The instance.
        instance: u8,
    },
    /// An IPv6 block too high to shift for the instance.
    #[error("networks.{network}.ipv6.cidr: {cidr} leaves no room for instance {instance}")]
    InstanceIpv6 {
        /// The network.
        network: String,
        /// Its IPv6 block.
        cidr: String,
        /// The instance.
        instance: u8,
    },
}

/// What a function of this crate returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Problems as an author reads them, one after the other.
struct Rendered<'a> {
    file: &'a str,
    problems: &'a [Problem],
}

impl fmt::Display for Rendered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, p) in self.problems.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            f.write_str(&p.render(self.file))?;
        }
        Ok(())
    }
}

/// Items one per line.
struct Lines<'a, T>(&'a [T]);

impl<T: fmt::Display> fmt::Display for Lines<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// The profiles a project has, or the folder that has none.
struct Known<'a> {
    folder: &'a PathBuf,
    known: &'a [String],
}

impl fmt::Display for Known<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.known.is_empty() {
            write!(f, "no profiles in {}", self.folder.display())
        } else {
            write!(f, "profiles here: {}", self.known.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{Code, FieldPath};

    #[test]
    fn a_specs_problems_read_one_after_the_other() {
        let problems = vec![
            Problem::new(Code::MissingField, FieldPath::new("version"), "a spec says its format's version"),
            Problem::new(Code::UnknownField, FieldPath::new("nme"), "`nme` isn't a field here"),
        ];
        let invalid = Error::Invalid {
            file: "isoloom.yaml".into(),
            problems,
        };
        assert_eq!(
            invalid.to_string(),
            "error[missing-field] version\n  a spec says its format's version\nerror[unknown-field] nme\n  `nme` isn't a field here"
        );
    }

    #[test]
    fn a_missing_profile_lists_the_profiles_there_are() {
        let missing = |known: &[&str]| Error::NoProfile {
            name: "c".into(),
            folder: PathBuf::from("/lab/profiles"),
            known: known.iter().map(|k| (*k).to_owned()).collect(),
        };
        assert_eq!(missing(&[]).to_string(), "no profile `c` (no profiles in /lab/profiles)");
        assert_eq!(missing(&["a", "b"]).to_string(), "no profile `c` (profiles here: a, b)");
    }

    #[test]
    fn a_profiles_problems_read_one_per_line() {
        let problems = vec![
            ProfileProblem::Input {
                profile: "p".into(),
                input: "A".into(),
            },
            ProfileProblem::Input {
                profile: "p".into(),
                input: "B".into(),
            },
        ];
        assert_eq!(
            Error::ProfileMismatch { problems }.to_string(),
            "profile `p`: input `A` isn't one of the spec's `inputs`\nprofile `p`: input `B` isn't one of the spec's `inputs`"
        );
    }

    #[test]
    fn several_sources_of_one_file_are_named_together() {
        assert_eq!(
            Error::FileSources(vec!["source", "input"]).to_string(),
            "`source` and `input` are both given: keep one source"
        );
        assert_eq!(
            Error::FileSources(vec!["source", "content", "input"]).to_string(),
            "`source` and `content` and `input` are all given: keep one source"
        );
    }
}
