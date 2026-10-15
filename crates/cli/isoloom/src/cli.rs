//! The command line's definition: the global options and one variant per subcommand, each
//! with its arguments in the subcommand's own module.

use clap::{Parser, Subcommand};

use crate::commands::{Listing, check, defaults, docs, generate, graph, import, inspect, message, report, schema, validate};

// No doc comments on the clap types themselves: clap would print them in `--help`.
#[derive(Debug, Parser)]
#[command(
    name = "isoloom",
    version,
    about = "Describe an environment once (machines, networks, ports), and get each tool's own files to run it: Compose, Kubernetes, Vagrant, Terraform, Ansible."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    /// Use a profile: `profiles/<name>.yml` in the project, what differs for one customer or
    /// site (target, spec values, defaults). Also `ISOLOOM_PROFILE`.
    #[arg(long, global = true, value_name = "NAME")]
    pub(crate) profile: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Check the spec for mistakes (and that every path it mentions exists).
    Validate(validate::Args),
    /// List the targets the spec can be generated for, and why the others can't.
    Targets(Listing),
    /// Add up the machines, CPUs, memory and disk the spec needs.
    Resources(Listing),
    /// Write each target's files under .isoloom/ (all possible targets, or one with --target).
    Generate(generate::Args),
    /// Fail when the generated files under .isoloom/ don't match the spec (for CI).
    Check(check::Args),
    /// The defaults in effect here (built in, your file, the project's, the environment) and
    /// where each comes from. See <https://www.isoloom.com/en/docs/defaults>
    Defaults(defaults::Args),
    /// Draw the environment: networks, machines with their addresses and ports, reach rules,
    /// gateways and the access machine. D2 by default; `--format dot` for Graphviz. With
    /// `-o x.svg` or `-o x.png` the diagram is rendered when `d2` (or `dot`) is installed.
    Graph(graph::Args),
    /// Print a table about the environment: `addressing`, `ports`, `wiring` or `resources`
    /// (text, or `--md` for Markdown). `isoloom report list` names them.
    Report(report::Args),
    /// Print the spec's `message` (how to use the environment), its placeholders filled.
    Message(message::Args),
    /// Show the resolved snapshot (what `.isoloom/resolved.json` holds): every address, routes,
    /// targets and checks, worked out from the spec. A dotted path narrows it:
    /// `isoloom inspect machines.web.addresses`, `isoloom inspect targets`.
    Inspect(inspect::Args),
    /// List the project's profiles (`profiles/<name>.yml`) and check each against the spec: the
    /// names it uses, the spec it makes, and that its target can run it. Fails when one doesn't.
    Profiles(Listing),
    /// Print the JSON Schema of isoloom.yml (for editors: completion, hover docs, errors).
    Schema(schema::Args),
    /// Draft an isoloom.yml from files you already have.
    #[command(subcommand)]
    Import(import::Import),
    /// Print a reference page generated from the code: `--spec` for docs/spec.md.
    Docs(docs::Args),
}
