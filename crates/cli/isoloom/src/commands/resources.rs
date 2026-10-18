//! `isoloom resources`: the machines, CPUs, memory and disk the spec needs.

use super::{Console, Listing, Result, Verdict, spec};

/// Prints the totals, as VMs.
pub(crate) fn run(a: &Listing, profile: &spec::Profile, console: &mut Console) -> Result<Verdict> {
    let lab = spec::lab(profile, &a.project.dir)?;
    let t = isoloom_spec::totals(&lab.spec());
    if a.json {
        console.json(&serde_json::json!({ "machines": t.machines, "cpus": t.cpus, "memoryMb": t.memory_mb, "diskGb": t.disk_gb }))?;
    } else {
        let memory_gb = f64::from(t.memory_mb) / 1024.0;
        console.line(format_args!(
            "{} machines · {} CPUs · {memory_gb:.1} GB memory · {} GB disk (as VMs)",
            t.machines, t.cpus, t.disk_gb
        ))?;
    }
    Ok(Verdict::Pass)
}
