//! Why a manifest, or one value in it, was refused: one enum for the crate, each message naming
//! where, why and what to write instead.

use crate::provenance::SUPPORTED_VERSION;

/// Why a manifest, or one of its values, was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The text isn't JSON.
    #[error("the manifest isn't JSON (line {line}, column {column}): {reason}; generate it again")]
    Syntax {
        /// The line the parser stopped at.
        line: usize,
        /// The column the parser stopped at.
        column: usize,
        /// What the JSON parser expected.
        reason: String,
    },
    /// Another version of the contract.
    #[error(
        "manifest_version {found}: this reader understands version {SUPPORTED_VERSION} only; generate the files with an Isoloom whose run manifest is version {SUPPORTED_VERSION}"
    )]
    Version {
        /// The version the manifest declares, as written.
        found: String,
    },
    /// A field is missing, unknown, or holds a value its type refuses.
    #[error("{path}: {reason}")]
    Field {
        /// Where, as `lifecycle.up[3].argv[0]`.
        path: String,
        /// Why, in the words of the value's own error.
        reason: String,
    },
    /// A name, an id or a version that doesn't match its pattern.
    #[error("`{value}` isn't a valid {what}: {rule}")]
    Pattern {
        /// What the value is (a step id, a machine name).
        what: &'static str,
        /// The value as written.
        value: String,
        /// The rule it breaks, as a reader would fix it.
        rule: &'static str,
    },
    /// A number outside its range.
    #[error("{value} is out of range: {what} goes from {min} to {max}")]
    OutOfRange {
        /// What the number is.
        what: &'static str,
        /// The number as written.
        value: u64,
        /// The smallest allowed.
        min: u64,
        /// The largest allowed.
        max: u64,
    },
    /// A shell or an interpreter as a step's program.
    #[error(
        "`{program}` is a shell or an interpreter, refused as a program: a step runs one tool without a shell; name the tool (`docker`), and pipe a guest script through `stdin`"
    )]
    ShellProgram {
        /// The program as written.
        program: String,
    },
    /// A program that isn't a bare tool name (a path, an option, a placeholder).
    #[error("`{program}` isn't a bare tool name: write the tool's name alone (`docker`); the runner finds it on PATH")]
    ProgramNotBare {
        /// The program as written.
        program: String,
    },
    /// An argv without its program.
    #[error("an argv is empty: it needs at least its program")]
    EmptyArgv,
    /// A per-host argv naming no host.
    #[error("a per-host argv names no host: give `default`, `linux`, `macos` or `windows`")]
    NoHostArgv,
    /// A `${secrets.…}` placeholder.
    #[error("`{template}`: `${{secrets.…}}` is refused; a secret reaches a tool only through its slot's delivery, never on a command line")]
    SecretPlaceholder {
        /// The template as written.
        template: String,
    },
    /// A placeholder outside the grammar.
    #[error(
        "`{template}`: `${{{placeholder}}}` isn't a placeholder; the roots are run.name, run.dir, run.state_dir, run.provider, run.ssh_key, run.known_hosts, run.ssh_public_key, run.slot, params.<NAME>, outputs.<name>, published.<machine>.<port>, machine and value"
    )]
    UnknownPlaceholder {
        /// The template as written.
        template: String,
        /// What is between the braces.
        placeholder: String,
    },
    /// A `$` that starts no placeholder.
    #[error("`{template}`: a `$` that starts no placeholder; write `$$` for a literal `$`")]
    LoneDollar {
        /// The template as written.
        template: String,
    },
    /// A `${` without its `}`.
    #[error("`{template}`: a `${{` without its `}}`")]
    UnclosedPlaceholder {
        /// The template as written.
        template: String,
    },
    /// An empty path.
    #[error("a path is empty: name a file or a folder relative to the manifest's folder")]
    EmptyPath,
    /// An absolute path where a relative one belongs.
    #[error("`{path}` is absolute: paths are relative to the manifest's folder (or start at `${{run.state_dir}}`)")]
    AbsolutePath {
        /// The path as written.
        path: String,
    },
    /// A path with a backslash.
    #[error("`{path}` has a `\\`: paths are `/`-separated on every host, and the runner converts them")]
    Backslash {
        /// The path as written.
        path: String,
    },
    /// A path that climbs out of its folder.
    #[error("`{path}` goes up with `..`: a path stays inside the folder it is relative to")]
    ParentPath {
        /// The path as written.
        path: String,
    },
    /// A path inside a machine that isn't absolute.
    #[error("`{path}` isn't absolute: a path inside a machine starts with `/`")]
    RelativeGuestPath {
        /// The path as written.
        path: String,
    },
    /// A network block that doesn't parse.
    #[error("`{value}` isn't a network block: write an address and a prefix length (`10.30.0.0/24`)")]
    Cidr {
        /// The block as written.
        value: String,
    },
    /// A list that must have an item but has none.
    #[error("{what} is empty: it needs at least one")]
    Empty {
        /// What the list is.
        what: &'static str,
    },
    /// `*` beside step ids in a delivery.
    #[error("`*` stands for every step on its own: list step ids, or `*` alone")]
    MixedWildcard,
    /// A step with both or neither of `argv` and `action`, or `with` without an action.
    #[error("step `{id}`: {reason}")]
    StepShape {
        /// The step's id.
        id: String,
        /// What is wrong with its command.
        reason: &'static str,
    },
    /// A Vagrant-style native name that is neither a Compose service nor a VM.
    #[error("`native` names a Compose `service`, or a `vagrant` machine with its `vm_names`, not both")]
    NativeShape,
    /// A UDP port marked `http` or `tls`.
    #[error("a UDP port speaks neither HTTP nor TLS, which run over TCP: set `protocol` to `tcp`, or `http` and `tls` to false")]
    PortProtocol,
    /// No host operating system for the target.
    #[error("hosts is empty: name the host operating systems the target runs from (`linux`, `macos`, `windows`)")]
    NoHosts,
    /// The identity isn't the driver's.
    #[error("driver `{driver}` with an identity for `{identity}`: write the identity of the driver that runs the manifest")]
    DriverIdentity {
        /// The manifest's driver.
        driver: &'static str,
        /// The identity's kind.
        identity: &'static str,
    },
    /// A step id used twice in one list.
    #[error("{list}: step id `{id}` appears twice; ids are unique within a list")]
    DuplicateStep {
        /// The list (`lifecycle.up`).
        list: String,
        /// The id.
        id: String,
    },
    /// A reference to a step that doesn't exist.
    #[error("{at}: no step `{step}` in this manifest")]
    UnknownStep {
        /// Where the reference is.
        at: String,
        /// The step it names.
        step: String,
    },
    /// A `step_ok` condition naming a step outside `lifecycle.up`.
    #[error("ready[{index}]: `{step}` isn't a step of lifecycle.up; `step_ok` names an up step")]
    ReadyNotUp {
        /// The condition's position in `ready`.
        index: usize,
        /// The step it names.
        step: String,
    },
    /// A machine named twice.
    #[error("machines: `{name}` appears twice")]
    DuplicateMachine {
        /// The name.
        name: String,
    },
    /// A port name used twice on one machine.
    #[error("machine `{machine}`: port `{port}` appears twice; a port's name is unique on its machine")]
    DuplicatePort {
        /// The machine.
        machine: String,
        /// The port's name.
        port: String,
    },
    /// A reference to a machine that doesn't exist.
    #[error("{at}: no machine `{machine}` in this manifest")]
    UnknownMachine {
        /// Where the reference is.
        at: String,
        /// The machine it names.
        machine: String,
    },
    /// A reference to a port no machine publishes.
    #[error("{at}: `${{published.{port}}}` names no published port; publish the port on that machine, or name one that is")]
    UnknownPublished {
        /// Where the reference is.
        at: String,
        /// The port, as `<machine>.<port>[.udp]`.
        port: String,
    },
    /// A `${params.…}` naming no param.
    #[error("{at}: `${{params.{name}}}` names no param; declare it in `params`")]
    UnknownParam {
        /// Where the placeholder is.
        at: String,
        /// The param it names.
        name: String,
    },
    /// A `${outputs.…}` naming no output.
    #[error("{at}: `${{outputs.{name}}}` names no output; declare it in `outputs`")]
    UnknownOutput {
        /// Where the placeholder is.
        at: String,
        /// The output it names.
        name: String,
    },
    /// A placeholder outside the only fields that bind it (`${value}`, `${machine}`).
    #[error("{at}: `${{{root}}}` is bound only in {only}")]
    PlaceholderOutOfPlace {
        /// Where the placeholder is.
        at: String,
        /// The placeholder's root.
        root: &'static str,
        /// The fields that bind it.
        only: &'static str,
    },
}

/// What reading or building a manifest returns.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// A stable code for the error, the same in Isoloom and the runner (`shell_program`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Error::Syntax { .. } => "syntax",
            Error::Version { .. } => "version",
            Error::Field { .. } => "field",
            Error::Pattern { .. } => "pattern",
            Error::OutOfRange { .. } => "out_of_range",
            Error::ShellProgram { .. } => "shell_program",
            Error::ProgramNotBare { .. } => "program_not_bare",
            Error::EmptyArgv => "empty_argv",
            Error::NoHostArgv => "no_host_argv",
            Error::SecretPlaceholder { .. } => "secret_placeholder",
            Error::UnknownPlaceholder { .. } => "unknown_placeholder",
            Error::LoneDollar { .. } => "lone_dollar",
            Error::UnclosedPlaceholder { .. } => "unclosed_placeholder",
            Error::EmptyPath => "empty_path",
            Error::AbsolutePath { .. } => "absolute_path",
            Error::Backslash { .. } => "backslash",
            Error::ParentPath { .. } => "parent_path",
            Error::RelativeGuestPath { .. } => "relative_guest_path",
            Error::Cidr { .. } => "cidr",
            Error::Empty { .. } => "empty",
            Error::MixedWildcard => "mixed_wildcard",
            Error::StepShape { .. } => "step_shape",
            Error::NativeShape => "native_shape",
            Error::PortProtocol => "port_protocol",
            Error::NoHosts => "no_hosts",
            Error::DriverIdentity { .. } => "driver_identity",
            Error::DuplicateStep { .. } => "duplicate_step",
            Error::UnknownStep { .. } => "unknown_step",
            Error::ReadyNotUp { .. } => "ready_not_up",
            Error::DuplicateMachine { .. } => "duplicate_machine",
            Error::DuplicatePort { .. } => "duplicate_port",
            Error::UnknownMachine { .. } => "unknown_machine",
            Error::UnknownPublished { .. } => "unknown_published",
            Error::UnknownParam { .. } => "unknown_param",
            Error::UnknownOutput { .. } => "unknown_output",
            Error::PlaceholderOutOfPlace { .. } => "placeholder_out_of_place",
        }
    }
}
