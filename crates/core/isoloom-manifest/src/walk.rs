//! Every list of steps and every template of a manifest, each with where it sits
//! (`lifecycle.up[3].argv[2]`): what validation checks, and what a runner renders.

use crate::argv::{Argv, HostArgv};
use crate::lifecycle::Condition;
use crate::step::{Run, Step};
use crate::template::Template;
use crate::{Manifest, PathTemplate};

impl Manifest {
    /// Every list of steps, with its location. The status probe is a list of one, and the check
    /// runners' steps are one list.
    #[must_use]
    pub fn step_lists(&self) -> Vec<(String, Vec<&Step>)> {
        let l = &self.lifecycle;
        let mut out: Vec<(String, Vec<&Step>)> = vec![
            ("lifecycle.up".into(), l.up.iter().collect()),
            ("lifecycle.down".into(), l.down.iter().collect()),
            ("lifecycle.resume".into(), l.resume.iter().collect()),
            ("lifecycle.seal.steps".into(), l.seal.iter().flat_map(|s| s.steps.iter()).collect()),
            ("lifecycle.status".into(), vec![&l.status]),
            ("checks.setup".into(), self.checks.setup.iter().collect()),
            ("checks.runners".into(), self.checks.runners.iter().map(|r| &r.step).collect()),
            ("checks.teardown".into(), self.checks.teardown.iter().collect()),
        ];
        if let Some(park) = &l.park {
            out.push(("lifecycle.park.pause".into(), park.pause.iter().flatten().collect()));
            out.push(("lifecycle.park.shutdown".into(), park.shutdown.iter().flatten().collect()));
        }
        if let Some(p) = &l.provision {
            out.push(("lifecycle.provision.all".into(), p.all.iter().collect()));
            out.push(("lifecycle.provision.machine".into(), p.machine.iter().collect()));
        }
        for (i, r) in self.checks.runners.iter().enumerate() {
            out.push((format!("checks.runners[{i}].on_timeout"), r.on_timeout.iter().collect()));
        }
        if let Some(r) = &self.recovery {
            out.push(("recovery.steps".into(), r.steps.iter().collect()));
        }
        if let Some(g) = &self.guest {
            out.push(("guest.status".into(), vec![&g.status]));
        }
        out
    }

    /// Every template, with its location.
    #[must_use]
    pub fn templates(&self) -> Vec<(String, &Template)> {
        let mut out: Vec<(String, &Template)> = Vec::new();
        if let Some(m) = &self.lab.message {
            out.push(("lab.message".into(), m));
        }
        if let crate::Identity::Compose(c) = &self.identity {
            out.push(("identity.compose.project".into(), &c.project));
            let files = c.files.iter().enumerate().map(|(i, f)| (format!("identity.compose.files[{i}]"), f));
            let pinned = c.pinned_ports_file.iter().map(|f| ("identity.compose.pinned_ports_file".to_owned(), f));
            out.extend(files.chain(pinned).map(|(at, f)| (at, PathTemplate::template(f))));
        }
        if let crate::Identity::Vagrant(v) = &self.identity {
            out.extend(v.dotfile.iter().map(|f| ("identity.vagrant.dotfile".to_owned(), f.template())));
        }
        for (i, t) in self.tools.iter().enumerate() {
            host_argv(&format!("tools[{i}].probe"), &t.probe, &mut out);
        }
        for (i, n) in self.networks.iter().enumerate() {
            out.extend(n.native.iter().map(|t| (format!("networks[{i}].native"), t)));
        }
        self.machine_templates(&mut out);
        for (list, steps) in self.step_lists() {
            for (i, s) in steps.iter().enumerate() {
                step(&format!("{list}[{i}]"), s, &mut out);
            }
        }
        for (i, c) in self.ready.iter().enumerate() {
            condition(&format!("ready[{i}]"), c, &mut out);
        }
        for (i, o) in self.outputs.iter().enumerate() {
            out.extend(o.format.iter().map(|t| (format!("outputs[{i}].format"), t)));
        }
        if let Some(host) = self.guest.as_ref().and_then(|g| g.host.as_ref()) {
            out.push(("guest.host".into(), host));
        }
        out
    }

    fn machine_templates<'a>(&'a self, out: &mut Vec<(String, &'a Template)>) {
        for (i, m) in self.machines.iter().enumerate() {
            for (j, v) in m.volumes.iter().enumerate() {
                out.extend(v.native.iter().map(|t| (format!("machines[{i}].volumes[{j}].native"), t)));
            }
            for (j, p) in m.ports.iter().enumerate() {
                let address = p.publish.as_ref().and_then(|p| p.address.as_ref());
                out.extend(address.map(|t| (format!("machines[{i}].ports[{j}].publish.address"), t)));
            }
        }
    }
}

fn argv<'a>(at: &str, a: &'a Argv, out: &mut Vec<(String, &'a Template)>) {
    out.extend(a.args().iter().enumerate().map(|(n, t)| (format!("{at}[{}]", n + 1), t)));
}

fn host_argv<'a>(at: &str, a: &'a HostArgv, out: &mut Vec<(String, &'a Template)>) {
    match a {
        HostArgv::Every(a) => argv(at, a, out),
        HostArgv::PerHost(p) => {
            for (host, per_host) in [("default", &p.default), ("linux", &p.linux), ("macos", &p.macos), ("windows", &p.windows)] {
                if let Some(one) = per_host {
                    argv(&format!("{at}.{host}"), one, out);
                }
            }
        }
    }
}

fn step<'a>(at: &str, s: &'a Step, out: &mut Vec<(String, &'a Template)>) {
    match &s.run {
        Run::Argv(a) => host_argv(&format!("{at}.argv"), a, out),
        Run::Action { with, .. } => out.extend(with.iter().map(|(k, t)| (format!("{at}.with.{k}"), t))),
    }
    out.extend(s.env.iter().map(|(k, t)| (format!("{at}.env.{k}"), t)));
    out.extend(s.stdin.iter().map(|p| (format!("{at}.stdin"), p.template())));
}

fn condition<'a>(at: &str, c: &'a Condition, out: &mut Vec<(String, &'a Template)>) {
    match c {
        Condition::StepOk { .. } | Condition::MachinesRunning { .. } => {}
        Condition::ReadyFile {
            host, user, path, log, via, ..
        } => {
            out.extend([("host", host), ("user", user), ("path", path)].map(|(k, t)| (format!("{at}.{k}"), t)));
            out.extend(log.iter().map(|t| (format!("{at}.log"), t)));
            out.extend(via.iter().map(|t| (format!("{at}.via"), t)));
        }
        Condition::Tcp { address, port, .. } => {
            out.extend([("address", address), ("port", port)].map(|(k, t)| (format!("{at}.{k}"), t)));
        }
    }
}
