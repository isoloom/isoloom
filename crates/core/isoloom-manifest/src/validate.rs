//! What the types can't say on their own because it spans fields: the driver and its identity,
//! unique step ids, references to steps, machines and published ports, and placeholders that
//! name something the manifest declares.

use std::collections::BTreeSet;

use crate::lifecycle::Condition;
use crate::outputs::OutputSource;
use crate::slots::Delivery;
use crate::template::Placeholder;
use crate::{Error, Manifest, Result, StepSelection};

impl Manifest {
    /// Checks what spans fields; the first problem found, if any.
    ///
    /// # Errors
    ///
    /// The first of: [`Error::NoHosts`], [`Error::DriverIdentity`] (an identity
    /// for another driver), [`Error::DuplicateStep`], [`Error::UnknownStep`] (a
    /// delivery's step), [`Error::ReadyNotUp`], [`Error::DuplicateMachine`],
    /// [`Error::DuplicatePort`], [`Error::UnknownMachine`],
    /// [`Error::UnknownPublished`], [`Error::UnknownParam`],
    /// [`Error::UnknownOutput`] and [`Error::PlaceholderOutOfPlace`].
    pub fn validate(&self) -> Result<()> {
        if self.hosts.is_empty() {
            return Err(Error::NoHosts);
        }
        if self.identity.driver() != self.driver {
            return Err(Error::DriverIdentity {
                driver: self.driver.id(),
                identity: self.identity.driver().id(),
            });
        }
        self.unique_steps()?;
        self.deliveries()?;
        self.ready_conditions()?;
        self.machine_references()?;
        self.placeholders()
    }

    fn unique_steps(&self) -> Result<()> {
        for (list, steps) in self.step_lists() {
            let mut seen = BTreeSet::new();
            if let Some(dup) = steps.iter().find(|s| !seen.insert(&s.id)) {
                return Err(Error::DuplicateStep { list, id: dup.id.to_string() });
            }
        }
        Ok(())
    }

    fn deliveries(&self) -> Result<()> {
        let lists = self.step_lists();
        let ids: BTreeSet<&str> = lists.iter().flat_map(|(_, steps)| steps.iter().map(|s| s.id.as_str())).collect();
        let params = self.params.iter().enumerate().map(|(i, p)| (format!("params[{i}]"), p.deliver.as_slice()));
        let slots = self.secrets.iter().enumerate().map(|(i, s)| (format!("secrets[{i}]"), s.deliver.as_slice()));
        for (at, deliveries) in params.chain(slots) {
            for (j, d) in deliveries.iter().enumerate() {
                let at = format!("{at}.deliver[{j}]");
                let named: Vec<&str> = match d {
                    Delivery::Env {
                        steps: StepSelection::Only(steps),
                        ..
                    } => steps.iter().map(crate::StepId::as_str).collect(),
                    Delivery::Guest {
                        during_step: Some(step),
                        machine,
                        ..
                    } => {
                        self.machine_exists(&at, machine.as_str())?;
                        vec![step.as_str()]
                    }
                    Delivery::Guest { machine, .. } => {
                        self.machine_exists(&at, machine.as_str())?;
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
                if let Some(missing) = named.into_iter().find(|s| !ids.contains(s)) {
                    return Err(Error::UnknownStep { at, step: missing.to_owned() });
                }
            }
        }
        Ok(())
    }

    fn ready_conditions(&self) -> Result<()> {
        for (index, c) in self.ready.iter().enumerate() {
            match c {
                Condition::StepOk { step, .. } if !self.lifecycle.up.iter().any(|s| &s.id == step) => {
                    return Err(Error::ReadyNotUp { index, step: step.to_string() });
                }
                Condition::MachinesRunning { machines } => {
                    for m in machines {
                        self.machine_exists(&format!("ready[{index}]"), m.as_str())?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn machine_references(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        if let Some(dup) = self.machines.iter().find(|m| !seen.insert(&m.name)) {
            return Err(Error::DuplicateMachine { name: dup.name.to_string() });
        }
        for m in &self.machines {
            let mut names = BTreeSet::new();
            if let Some(dup) = m.ports.iter().find(|p| !names.insert(&p.name)) {
                return Err(Error::DuplicatePort {
                    machine: m.name.to_string(),
                    port: dup.name.to_string(),
                });
            }
        }
        for (i, m) in self.machines.iter().enumerate() {
            if let Some(of) = &m.of {
                self.machine_exists(&format!("machines[{i}].of"), of.as_str())?;
            }
        }
        for (i, s) in self.secrets.iter().enumerate() {
            for m in &s.machines {
                self.machine_exists(&format!("secrets[{i}].machines"), m.as_str())?;
            }
        }
        for (i, o) in self.outputs.iter().enumerate() {
            if let OutputSource::PublishedPort { machine, port } = &o.from {
                self.published(&format!("outputs[{i}].from"), machine.as_str(), port.as_str())?;
            }
        }
        Ok(())
    }

    fn placeholders(&self) -> Result<()> {
        for (at, template) in self.templates() {
            for p in template.placeholders() {
                match p {
                    Placeholder::Published { machine, port } => self.published(&at, machine.as_str(), port.as_str())?,
                    Placeholder::Host(machine) => self.machine_exists(&at, machine.as_str())?,
                    Placeholder::Param(name) if !self.params.iter().any(|x| &x.name == name) => {
                        return Err(Error::UnknownParam { at, name: name.to_string() });
                    }
                    Placeholder::Output(name) if !self.outputs.iter().any(|x| &x.name == name) => {
                        return Err(Error::UnknownOutput { at, name: name.to_string() });
                    }
                    // An output's only template is its `format`.
                    Placeholder::Value if !at.starts_with("outputs[") => {
                        return Err(out_of_place(at, "value", "an output's `format`"));
                    }
                    Placeholder::Machine if !at.starts_with("lifecycle.provision.machine[") => {
                        return Err(out_of_place(at, "machine", "the steps of `lifecycle.provision.machine`"));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn machine_exists(&self, at: &str, machine: &str) -> Result<()> {
        if self.machines.iter().any(|m| m.name.as_str() == machine) {
            Ok(())
        } else {
            Err(Error::UnknownMachine {
                at: at.to_owned(),
                machine: machine.to_owned(),
            })
        }
    }

    /// Whether `machine` publishes its port named `port`.
    fn published(&self, at: &str, machine: &str, port: &str) -> Result<()> {
        let found = self
            .machines
            .iter()
            .filter(|m| m.name.as_str() == machine)
            .flat_map(|m| &m.ports)
            .any(|p| p.name.as_str() == port && p.publish.is_some());
        if found {
            return Ok(());
        }
        Err(Error::UnknownPublished {
            at: at.to_owned(),
            port: format!("{machine}.{port}"),
        })
    }
}

fn out_of_place(at: String, root: &'static str, only: &'static str) -> Error {
    Error::PlaceholderOutOfPlace { at, root, only }
}

#[cfg(test)]
mod tests;
