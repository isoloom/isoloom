//! Each machine of the environment, planned: its shape, addresses, routes, files, settings,
//! image and recipe.

use indexmap::IndexMap;
use isoloom_spec::images;
use isoloom_spec::model::{Constraints, Machine, MachineFile, Spec, Target};
use isoloom_spec::net::Rate;
use isoloom_spec::windows;

use super::{MachinePlan, MachineRole, MachineShape, PlanError, PublishedPort, Settings, image, shape};
use crate::addressing::address;
use crate::configure::{DaemonTable, Recipe};
use crate::hardware::Hardware;
use crate::process::Process;
use crate::secrets::SecretFile;
use crate::{ansible, ipv6, lifecycle, outside, routing, storage};

/// The machines of `laid_out`, each with its place on the outside networks of `spec`.
pub(super) fn machines(spec: &Spec, laid_out: &Spec, target: Target, daemons: &DaemonTable) -> Result<IndexMap<String, MachinePlan>, PlanError> {
    laid_out
        .machines
        .iter()
        .map(|(name, m)| {
            let mut mp = machine(laid_out, target, name, m, daemons)?;
            mp.outside = spec.machines.get(name).map(|om| outside::addresses(spec, om)).unwrap_or_default();
            Ok((name.clone(), mp))
        })
        .collect()
}

/// Machine `name`'s recipe, when it has a configuration.
fn recipe(name: &str, m: &Machine, daemons: &DaemonTable) -> Result<Option<Recipe>, PlanError> {
    m.configuration
        .as_ref()
        .map(|c| Recipe::of(c, &m.ports, daemons))
        .transpose()
        .map_err(|unportable| PlanError::UnresolvedRecipe {
            machine: name.to_owned(),
            unportable,
        })
}

/// Machine `name`'s `rate_limit`, read once here (validation refuses one that isn't a rate).
fn rate_limit(name: &str, m: &Machine) -> Result<Option<Rate>, PlanError> {
    m.rate_limit
        .as_deref()
        .map(|rate| {
            Rate::parse(rate).map_err(|error| PlanError::RateLimit {
                machine: name.to_owned(),
                rate: rate.to_owned(),
                error,
            })
        })
        .transpose()
}

/// Machine `name` (`m`), planned for `target`. Lowering refuses a configuration that doesn't
/// resolve (an app's steps per target, a service's for the environment), so the lowered spec `build`
/// plans has its recipe; one that doesn't is an error, not a machine with no configuration.
fn machine(spec: &Spec, target: Target, name: &str, m: &Machine, daemons: &DaemonTable) -> Result<MachinePlan, PlanError> {
    let shape = shape(target, m);
    let routing = lifecycle::Routing::of(spec, target, name, m);
    let recipe = recipe(name, m, daemons)?;
    Ok(MachinePlan {
        shape,
        role: if m.access { MachineRole::Access } else { MachineRole::Member },
        addresses: m.networks.iter().map(|(n, o)| (n.clone(), address(spec, n, *o))).collect(),
        more_addresses: crate::more_addresses::of(spec, m),
        addresses6: ipv6::addresses(spec, m),
        names: crate::private_dns::names(spec, name, m),
        lifecycle: if shape == MachineShape::Vm {
            lifecycle::of(spec, name, m, recipe.as_ref(), &routing)
        } else {
            Vec::new()
        },
        routes: routing.routes,
        routes6: routing.routes6,
        default_gateway: routing.default,
        is_gateway: routing::is_gateway(spec, name),
        offline: routing::offline(spec, name, m),
        seal: m.seal,
        ports: published(m),
        volumes: storage::volumes(spec, name, m),
        files: files(spec, m, shape),
        disks: storage::disks(name, m),
        host_folders: m.host_folders.clone(),
        settings: Settings {
            environment: m.environment.clone(),
            sysctls: m.sysctls.clone(),
            ulimits: m.ulimits.clone(),
        },
        inputs: m.inputs.clone(),
        secrets: m.secrets.iter().map(|s| SecretFile::of(spec, s)).collect(),
        resources: m.resources.unwrap_or_default(),
        arch: m.arch,
        platform: m.platform,
        rate_limit: rate_limit(name, m)?,
        image: image::of(spec, shape, name, m, recipe.as_ref()),
        vm_image: m.vm.as_ref().and_then(|v| v.image.clone()),
        boot: m.vm.as_ref().and_then(|v| v.boot.clone()),
        depends_on: m.depends_on.clone(),
        os: m.vm.as_ref().map(|v| v.os.clone()),
        winrm: m.vm.as_ref().filter(|v| images::is_windows(&v.os) && !windows::winssh(v)).map(images::winrm),
        winssh: m.vm.as_ref().is_some_and(windows::winssh),
        tpm: m.vm.as_ref().and_then(windows::tpm),
        external: m.external.clone(),
        host_vars: ansible::host_vars(spec, name),
        outside: IndexMap::new(),
        process: Process::of(m),
        hardware: Hardware::of(m),
        recipe,
    })
}

fn published(m: &Machine) -> Vec<PublishedPort> {
    m.ports
        .iter()
        .filter_map(|s| {
            s.publish.map(|publish| PublishedPort {
                name: m.port_label(s),
                port: s.port,
                publish,
                protocol: s.protocol,
                fixed: s.fixed,
            })
        })
        .collect()
}

/// A machine's files: the spec's, and for a container the certificate authorities it trusts
/// (`constraints.ca_certificates`) where its trust store reads them (a VM installs them in a
/// set-up step instead).
fn files(spec: &Spec, m: &Machine, shape: MachineShape) -> Vec<MachineFile> {
    let authorities = spec.constraints.iter().flat_map(|c| c.ca_certificates.iter()).enumerate();
    let trusted = authorities.filter(|_| shape == MachineShape::Container).map(|(n, source)| MachineFile {
        path: Constraints::ca_certificate_path(n),
        source: Some(source.clone()),
        content: None,
        uid: None,
        gid: None,
        mode: None,
    });
    m.files.iter().cloned().chain(trusted).collect()
}
