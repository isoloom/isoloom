//! The examples' manifests and charts as snapshots, a fixture's own process, and their
//! determinism.

use std::path::Path;

use isoloom_kubernetes::KubernetesBackend;
use isoloom_render::backend::Backend;

use crate::testing::{Outcome, chart, joined, manifests, parsed, render};

#[test]
fn a_machine_runs_its_own_process() -> Outcome {
    let fixture = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/own-process.yml"))?;
    let files = KubernetesBackend.render(&parsed(&fixture)?)?;
    insta::assert_snapshot!(joined(&files, |p| p.ends_with("environment.yaml")));
    Ok(())
}

/// The examples whose manifests and chart a reviewer reads whole, each its snapshot.
#[test]
fn each_examples_manifests_and_chart_are_their_snapshots() -> Outcome {
    for (name, example) in [("hello_stack", "hello-stack"), ("segmented", "segmented"), ("shared_files", "shared-files")] {
        insta::assert_snapshot!(format!("{name}_manifests"), manifests(example)?);
        insta::assert_snapshot!(format!("{name}_chart"), chart(example)?);
    }
    Ok(())
}

#[test]
fn rendering_twice_gives_the_same_files() -> Outcome {
    for example in [
        "hello-stack",
        "segmented",
        "shared-files",
        "supplier-portal-api",
        "invoice-portal-api",
        "udp-dns",
    ] {
        assert_eq!(render(example)?, render(example)?, "{example}");
    }
    Ok(())
}
