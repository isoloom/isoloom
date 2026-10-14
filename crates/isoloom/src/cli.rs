//! The command line's definition: the global options and one variant per subcommand, each
//! with its arguments in the subcommand's own module.

use clap::{Parser, Subcommand};

use crate::commands::{
    capture, chart, check, connect, coverage, defaults, docs, doctor, down, exec, gc, generate, graph, import, inspect, message, profiles, provision, report,
    reset, resources, run, schema, status, targets, tc, test, validate,
};

#[derive(Parser)]
#[command(
    name = "isoloom",
    version,
    about = "Describe an environment once (machines, networks, ports), run it anywhere: Docker, local VMs, Proxmox, cloud."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    /// Run with a profile: `profiles/<name>.yml` in the project, what differs for one customer or
    /// site (target, spec values, defaults, inputs). Also `ISOLOOM_PROFILE`.
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check the spec for mistakes (and that every path it mentions exists).
    Validate(validate::Args),
    /// List the targets the spec can run on, and why the others can't.
    Targets(targets::Args),
    /// What this machine can run: for every target, the tools and credentials found and the ones
    /// missing. No spec needed; with one in DIR, also what its cloud services require.
    Doctor(doctor::Args),
    /// Add up the machines, CPUs, memory and disk the spec needs.
    Resources(resources::Args),
    /// Write each target's files under .isoloom/ (all possible targets, or one with --target).
    Generate(generate::Args),
    /// Fail when the generated files under .isoloom/ don't match the spec (for CI).
    Check(check::Args),
    /// The defaults in effect here (built in, your file, the project's, the environment) and
    /// where each comes from. See <https://www.isoloom.com/en/docs/defaults>
    Defaults(defaults::Args),
    /// Generate a target's files, then bring the environment up with the right tool
    /// (docker compose, vagrant, kubectl, or terraform).
    Run(run::Args),
    /// Back to the environment as it came up, everything done in it since gone (files dropped,
    /// users added, databases changed): VMs restore the baseline snapshot `run` saved; without
    /// one, and on the other targets, the environment is torn down and run again (Docker: fresh
    /// containers and volumes from the same images).
    Reset(reset::Args),
    /// Tear down what `run` started for a target (the inverse tool: compose down, vagrant
    /// destroy, kubectl delete, or terraform destroy).
    Down(down::Args),
    /// Run the environment's checks against a running target: the spec's (scripts and declared
    /// probes) and the ones Isoloom derives from `ports` and `reach`. One line per check;
    /// exits 1 when any fails.
    Test(test::Args),
    /// Run the environment's provisioning again on a running environment, in place: after a
    /// step failed or was interrupted (a host that stopped answering), without a rebuild. With
    /// `provision:` steps, the controller runs them again (booted if halted, halted after),
    /// limited to the named machines; without, the named machines' own `vm.provision` steps run
    /// again. Steps are meant to be idempotent. Local VMs (vagrant, docker-vm, hybrid) for now.
    Provision(provision::Args),
    /// Docker leftovers of environments: networks and volumes labelled by Isoloom whose Compose
    /// project has no container left, and (with --images) built images no container uses.
    /// Lists them; `--yes` removes them. Never touches containers or unlabelled resources.
    Gc(gc::Args),
    /// The environments `run` brought up on this host, with their live state.
    Status(status::Args),
    /// Open a shell on a machine of a running environment (docker compose exec, vagrant ssh,
    /// kubectl exec, or ssh for the cloud targets).
    Connect(connect::Args),
    /// Run a command on a machine, or on every machine (`all`), of a running environment.
    Exec(exec::Args),
    /// Capture packets on a machine's interface on one of its networks (tcpdump, from inside
    /// the machine's network namespace).
    Capture(capture::Args),
    /// Draw the environment: networks, machines with their addresses and ports, reach rules,
    /// gateways and the access machine. D2 by default; `--format dot` for Graphviz. With
    /// `-o x.svg` or `-o x.png` the diagram is rendered when `d2` (or `dot`) is installed.
    Graph(graph::Args),
    /// Print a table about the environment: `addressing`, `ports`, `wiring` or `resources`
    /// (text, or `--md` for Markdown). `isoloom report list` names them.
    Report(report::Args),
    /// Link impairment on a running environment's router: `show`, `set` (with `--delay`, `--jitter`,
    /// `--loss`, `--rate`) or `disable` on a network, or `reset` to the spec's `tc`.
    Tc(tc::Args),
    /// Print the spec's `message` (how to use the environment), its placeholders filled.
    Message(message::Args),
    /// Show the resolved snapshot (what `.isoloom/resolved.json` holds): every address, routes,
    /// targets and checks, worked out from the spec. A dotted path narrows it:
    /// `isoloom inspect machines.web.addresses`, `isoloom inspect targets`.
    Inspect(inspect::Args),
    /// List the project's profiles (`profiles/<name>.yml`) and check each against the spec: the
    /// names it uses, the spec it makes, and that its target can run it. Fails when one doesn't.
    Profiles(profiles::Args),
    /// Package the Helm chart (`.isoloom/kubernetes/chart/`, written first) as `<name>-<version>.tgz`
    /// with `helm package`: the artifact to hand over or push to a chart registry.
    Chart(chart::Args),
    /// Print the JSON Schema of isoloom.yml (for editors: completion, hover docs, errors).
    Schema(schema::Args),
    /// Draft an isoloom.yml from files you already have.
    #[command(subcommand)]
    Import(import::Import),
    /// Show which features of each output format (Compose, Vagrant) a spec can produce.
    Coverage(coverage::Args),
    /// Print a reference page generated from the code: `--spec` for docs/spec.md.
    Docs(docs::Args),
}

impl Command {
    /// Whether the command was asked for JSON events and errors (`--json` on a lifecycle
    /// command). The other commands' `--json` prints one document of their own.
    pub fn json_events(&self) -> bool {
        match self {
            Command::Run(a) => a.json,
            Command::Reset(a) => a.json,
            Command::Down(a) => a.json,
            Command::Test(a) => a.json,
            Command::Provision(a) => a.json,
            Command::Gc(a) => a.json,
            Command::Status(a) => a.json,
            Command::Connect(a) => a.json,
            Command::Exec(a) => a.json,
            Command::Capture(a) => a.json,
            Command::Tc(a) => a.json,
            _ => false,
        }
    }
}
