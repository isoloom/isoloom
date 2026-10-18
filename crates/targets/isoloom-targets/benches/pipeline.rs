//! The hot paths of `isoloom generate` (PLAN 3d), on representative examples: parsing the spec,
//! lowering it into the plan of every target it is generated for, and rendering those plans into
//! files; and generating every example on every target, the generation-time budget's measure.
//! `cargo xtask bench` runs them, compares each one's median with the stored baseline
//! (`bench-baseline.json`), and holds `generate/every-example` to its budget.

use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::Criterion;
use isoloom_plan::Environment;
use isoloom_spec::Target;
use isoloom_targets::{Lab, backend, generate_all};

/// The examples measured: containers and VMs side by side (hello-stack), a router between three
/// networks (segmented), Windows machines in a domain (corp-ad-basics).
const EXAMPLES: [&str; 3] = ["hello-stack", "segmented", "corp-ad-basics"];

/// The repository's examples folder.
fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples")
}

/// One example, read and parsed once: what each benchmark starts from.
struct Example {
    name: &'static str,
    text: String,
    lab: Lab,
    targets: Vec<Target>,
}

fn example(name: &'static str) -> Result<Example, String> {
    let path = examples().join(name).join("isoloom.yml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lab = Lab::new(isoloom_spec::parse(&text).map_err(|e| format!("{name}: {e}"))?);
    let targets = lab.possible().into_iter().filter(|t| backend(*t).is_some()).collect();
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

/// Every example, read: each one's spec text.
fn every_example() -> Result<Vec<String>, String> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(examples())
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.join("isoloom.yml").is_file())
        .collect();
    dirs.sort();
    dirs.iter()
        .map(|d| std::fs::read_to_string(d.join("isoloom.yml")).map_err(|e| format!("{}: {e}", d.display())))
        .collect()
}

/// Every example parsed and generated for every target it allows: what `cargo xtask bench`
/// holds to the generation-time budget.
fn measure_everything(c: &mut Criterion, specs: &[String]) {
    let mut group = c.benchmark_group("generate");
    group.sample_size(10);
    group.bench_function("every-example", |b| {
        b.iter(|| {
            specs
                .iter()
                .filter_map(|text| isoloom_spec::parse(black_box(text)).ok())
                .map(|spec| generate_all(&Lab::new(spec)).0.len())
                .sum::<usize>()
        });
    });
    group.finish();
}

fn main() -> Result<(), String> {
    let examples = EXAMPLES.into_iter().map(example).collect::<Result<Vec<_>, _>>()?;
    let specs = every_example()?;
    let mut c = Criterion::default().configure_from_args();
    for e in &examples {
        measure(&mut c, e);
    }
    measure_everything(&mut c, &specs);
    c.final_summary();
    Ok(())
}
