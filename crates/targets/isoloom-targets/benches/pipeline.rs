//! The hot paths of `isoloom generate` (PLAN 3d), on representative examples: parsing the spec,
//! lowering it into the plan of every target it is generated for, and rendering those plans into
//! files. `cargo xtask bench` runs them and compares each one's median with the stored baseline
//! (`bench-baseline.json`).

use std::hint::black_box;
use std::path::Path;

use criterion::Criterion;
use isoloom_spec::Target;
use isoloom_targets::{Environment, GENERATED_TARGETS, Lab, backend};

/// The examples measured: containers and VMs side by side (hello-stack), a router between three
/// networks (segmented), Windows machines in a domain (corp-ad-basics).
const EXAMPLES: [&str; 3] = ["hello-stack", "segmented", "corp-ad-basics"];

/// One example, read and parsed once: what each benchmark starts from.
struct Example {
    name: &'static str,
    text: String,
    lab: Lab,
    targets: Vec<Target>,
}

fn example(name: &'static str) -> Result<Example, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples").join(name).join("isoloom.yml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lab = Lab::new(isoloom_spec::parse(&text).map_err(|e| format!("{name}: {e}"))?);
    let targets = lab.possible().into_iter().filter(|t| GENERATED_TARGETS.contains(t)).collect();
    Ok(Example { name, text, lab, targets })
}

fn measure(c: &mut Criterion, e: &Example) {
    c.bench_function(&format!("parse/{}", e.name), |b| b.iter(|| isoloom_spec::parse(black_box(&e.text))));
    // Lowering the authored spec for the target, then planning it.
    let lower = || {
        e.targets
            .iter()
            .map(|t| Environment::build(&black_box(&e.lab).for_target(*t), *t, None))
            .collect::<Vec<_>>()
    };
    c.bench_function(&format!("lower/{}", e.name), |b| b.iter(lower));
    let plans: Vec<(Target, Environment)> = e.targets.iter().copied().zip(lower()).collect();
    let render = || {
        plans
            .iter()
            .filter_map(|(t, env)| backend(*t).map(|b| b.render(black_box(env))))
            .collect::<Vec<_>>()
    };
    c.bench_function(&format!("render/{}", e.name), |b| b.iter(render));
}

fn main() -> Result<(), String> {
    let examples = EXAMPLES.into_iter().map(example).collect::<Result<Vec<_>, _>>()?;
    let mut c = Criterion::default().configure_from_args();
    for e in &examples {
        measure(&mut c, e);
    }
    c.final_summary();
    Ok(())
}
