//! Where each lowered field came from: a problem found on the lowered spec (its networks,
//! addresses, volumes, checks) is reported at the field the author wrote. Indexed fields
//! (a machine's ports, files, steps) are recorded as lowering writes them; renamed ones
//! (`docker.image` from `app.image`, `vm.os` from `os`) follow fixed rules.

use isoloom_spec::syntax::{self, FieldPath, Step};

/// Lowered paths back to written ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trail {
    /// (lowered prefix, written path), the longest prefix wins.
    entries: Vec<(String, String)>,
}

impl Trail {
    fn record(&mut self, lowered: String, written: String) {
        self.entries.push((lowered, written));
    }

    /// A machine's `kind` (`files` or `secrets`) item `lowered` came from its `files[written]`.
    pub(super) fn file(&mut self, machine: &str, kind: &str, lowered: usize, written: usize) {
        self.record(format!("machines.{machine}.{kind}[{lowered}]"), format!("machines.{machine}.files[{written}]"));
    }

    /// A machine's ports, steps, setup scripts and environment inputs.
    pub(super) fn machine(&mut self, name: &str, m: &syntax::Machine) {
        let at = format!("machines.{name}");
        for (i, p) in m.ports.keys().enumerate() {
            self.record(format!("{at}.ports[{i}]"), format!("{at}.ports.{p}"));
        }
        if let Some(app) = m.app() {
            let vm_steps = app.steps.iter().enumerate().filter(|(_, s)| !matches!(s, Step::Copy(_) | Step::Packages(_)));
            for (lowered, (written, _)) in vm_steps.enumerate() {
                self.record(format!("{at}.vm.provision[{lowered}]"), format!("{at}.app.steps[{written}]"));
            }
        }
        for i in 0..m.setup.len() {
            self.record(format!("{at}.docker.init[{i}]"), format!("{at}.setup[{i}]"));
        }
        let inputs = m.environment.iter().filter(|(_, v)| matches!(v, syntax::EnvValue::Input(_)));
        for (i, (var, _)) in inputs.enumerate() {
            self.record(format!("{at}.inputs[{i}]"), format!("{at}.environment.{var}"));
        }
    }

    /// The spec's inputs, and the secrets lowering made of the files they fill.
    pub(super) fn inputs(&mut self, source: &syntax::Spec) {
        for (i, name) in source.inputs.0.keys().enumerate() {
            self.record(format!("inputs[{i}]"), format!("inputs.{name}"));
            self.record(format!("secrets.{}", super::machine::input_secret(name.as_str())), format!("inputs.{name}"));
        }
    }

    /// The written path of lowered path `path`.
    pub fn path(&self, path: &str) -> FieldPath {
        let within = |prefix: &str| path == prefix || path.strip_prefix(prefix).is_some_and(|r| r.starts_with(['.', '[']));
        let best = self
            .entries
            .iter()
            .filter(|(lowered, _)| within(lowered))
            .max_by_key(|(lowered, _)| lowered.len());
        match best {
            Some((lowered, written)) => FieldPath::new(format!("{written}{}", rename_inside(&path[lowered.len()..]))),
            None => FieldPath::new(rename(path)),
        }
    }
}

/// The written name of a field inside a recorded one (a port's `publish`, a file's `target`).
fn rename_inside(rest: &str) -> String {
    match rest {
        ".http" | ".tls" => ".protocol".into(),
        ".fixed" => ".publish".into(),
        ".target" => ".path".into(),
        ".source" => String::new(),
        other => other.to_string(),
    }
}

/// The written path of a lowered one with no recorded index: the fixed renames.
fn rename(path: &str) -> String {
    let parts: Vec<&str> = path.splitn(3, '.').collect();
    match parts.as_slice() {
        ["machines", m, rest] => machine_field(m, rest),
        ["networks", n, rest] => match rest.split_once('.').map_or((*rest, ""), |(a, b)| (a, b)) {
            ("docker", "") => format!("overrides.docker.networks.{n}"),
            ("docker", inner) => format!("overrides.docker.networks.{n}.{inner}"),
            ("external", _) => format!("overrides.docker.networks.{n}.external"),
            _ => path.to_string(),
        },
        ["volumes", v, "external"] => format!("overrides.docker.volumes.{v}.external"),
        ["volumes", v, rest] if rest.starts_with("virtiofs") => format!("overrides.proxmox.volumes.{v}.{rest}"),
        ["controller", "image", ..] => "overrides.vagrant.controller".into(),
        _ => path.to_string(),
    }
}

/// A machine field's written path.
fn machine_field(m: &str, rest: &str) -> String {
    let at = format!("machines.{m}");
    let (head, tail) = rest.split_once(['.', '[']).map_or((rest, ""), |(h, _)| (h, &rest[h.len()..]));
    let image = |key: &str| -> String {
        match key {
            "vagrant" => format!("overrides.vagrant.machines.{m}.box"),
            "vagrant_version" => format!("overrides.vagrant.machines.{m}.box_version"),
            "qemu" | "hyperv" => format!("overrides.vagrant.machines.{m}.boxes.{key}"),
            "box_url" | "box_server_url" | "winrm" | "winssh" => format!("overrides.vagrant.machines.{m}.{key}"),
            "proxmox" => format!("overrides.proxmox.machines.{m}.image"),
            "" => format!("overrides.vagrant.machines.{m}"),
            cloud => format!("overrides.{cloud}.machines.{m}.image"),
        }
    };
    match (head, tail) {
        ("docker", t) => match t.trim_start_matches('.').split(['.', '[']).next().unwrap_or_default() {
            "image" | "appliance" | "config" | "firmware" => format!("{at}.app{t}"),
            "build" | "dockerfile" | "target" | "args" => format!("{at}.app.build"),
            "init" => format!("{at}.setup{}", t.trim_start_matches(".init")),
            "lxc" => format!("overrides.proxmox.machines.{m}.lxc"),
            "idle" => format!("{at}.os"),
            _ => format!("{at}.app"),
        },
        ("vm", t) => match t
            .trim_start_matches('.')
            .split_once('.')
            .map_or((t.trim_start_matches('.'), ""), |(a, b)| (a, b))
        {
            ("os" | "", _) => format!("{at}.os"),
            ("image", key) => image(key),
            ("tpm", _) => format!("{at}.hardware.tpm"),
            ("boot", "") => format!("{at}.hardware.boot"),
            ("boot", inner) => format!("{at}.hardware.boot.{inner}"),
            (field, _) if field.starts_with("provision") => format!("{at}.app.steps{}", field.trim_start_matches("provision")),
            _ => format!("{at}.app"),
        },
        ("arch" | "gpus" | "devices" | "nested_virtualization", t) => format!("{at}.hardware.{head}{t}"),
        ("working_dir", _) => format!("{at}.command.cwd"),
        ("external", _) => format!("overrides.external.machines.{m}.ssh"),
        ("more_addresses", t) => format!("{at}.networks{t}"),
        _ => format!("{at}.{rest}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renamed_fields_point_at_what_the_author_wrote() {
        let t = Trail::default();
        for (lowered, written) in [
            ("machines.web.docker.image", "machines.web.app.image"),
            ("machines.web.docker.dockerfile", "machines.web.app.build"),
            ("machines.web.docker.init[1]", "machines.web.setup[1]"),
            ("machines.web.docker.lxc", "overrides.proxmox.machines.web.lxc"),
            ("machines.web.vm.os", "machines.web.os"),
            ("machines.web.vm.tpm", "machines.web.hardware.tpm"),
            ("machines.web.vm.image.vagrant", "overrides.vagrant.machines.web.box"),
            ("machines.web.vm.image.aws", "overrides.aws.machines.web.image"),
            ("machines.web.arch", "machines.web.hardware.arch"),
            ("machines.web.working_dir", "machines.web.command.cwd"),
            ("machines.web.networks.lab", "machines.web.networks.lab"),
            ("networks.lab.docker.cidr", "overrides.docker.networks.lab.cidr"),
            ("volumes.data.virtiofs.cache", "overrides.proxmox.volumes.data.virtiofs.cache"),
            ("controller.image.vagrant", "overrides.vagrant.controller"),
            ("checks[2].from", "checks[2].from"),
        ] {
            assert_eq!(t.path(lowered).as_str(), written, "{lowered}");
        }
    }

    #[test]
    fn recorded_items_point_at_their_written_index() {
        let mut t = Trail::default();
        t.file("web", "secrets", 0, 2);
        t.record("machines.web.ports[0]".into(), "machines.web.ports.site".into());
        assert_eq!(t.path("machines.web.secrets[0].target").as_str(), "machines.web.files[2].path");
        assert_eq!(t.path("machines.web.ports[0].fixed").as_str(), "machines.web.ports.site.publish");
        assert_eq!(t.path("machines.web.ports[0]").as_str(), "machines.web.ports.site");
    }
}
