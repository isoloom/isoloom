//! What the controller waits for before it runs the playbooks: each machine's set-up done.

use isoloom_plan::{Environment, Vm};
use isoloom_render::inventory::{CONTROLLER_INVENTORY, CONTROLLER_KEY};

use crate::machine::Shell;

/// Each Linux machine's own set-up must be done (its ready file says `ready`) before the
/// playbooks run: its name, and the wait, over SSH with the controller's key as `user` of its OS at the
/// machine's first address.
pub(crate) fn linux_waits(env: &Environment, user: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
    env.vms()
        .filter(|vm| !vm.plan.is_windows())
        .filter_map(|Vm { name, plan: mp, os }| {
            let a = mp.addresses.values().next()?;
            let u = user(os);
            let ready = isoloom_render::steps::READY_FILE;
            // Its ready file's first line: `ready` goes on, `failed: <step>` stops the playbooks.
            let (done, failed) = (isoloom_spec::ready_line::ReadyLine::READY, isoloom_render::steps::failed_pattern());
            let wait = format!(
                "i=0; until s=$(ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i {CONTROLLER_KEY} {u}@{a} head -n 1 {ready} 2>/dev/null) && [ \"$s\" = {done} ]; do case \"$s\" in {failed}) echo \"{name}: $s\"; exit 1;; esac; i=$((i+1)); [ $i -gt 240 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 5; done"
            );
            Some((name.to_owned(), Shell::Root.command(&wait)))
        })
        .collect()
}

/// The controller's wait for each Windows machine's ready marker, through Ansible over WinRM
/// (the inventory's own connection), before the playbooks run: a Windows machine set up at its
/// first boot (cloudbase-init) has no set-up of Terraform's to wait for.
pub(crate) fn windows_waits(env: &Environment) -> Vec<String> {
    env.vms()
        .filter(|vm| vm.plan.is_windows())
        .map(|Vm { name, .. }| {
            format!(
                "i=0; until /opt/ansible/bin/ansible -i {CONTROLLER_INVENTORY} {name} -m raw -a 'if (-not (Test-Path {ready})) {{ exit 1 }}' >/dev/null 2>&1; do i=$((i+1)); [ $i -gt 360 ] && {{ echo '{name} not ready'; exit 1; }}; sleep 10; done",
                ready = isoloom_spec::windows::READY_FILE
            )
        })
        .collect()
}
