//! Helm: the fields of `Chart.yaml` (apiVersion v2), from SchemaStore's pinned schema
//! (`coverage/kubernetes/chart.txt`, see `scripts/refresh-kubernetes.py`), then the chart's
//! features: the files a chart may hold and the hooks its templates may declare, as Helm 3.16's
//! documentation lists them ("The Chart File Structure", "Chart Hooks", "Chart Tests").
//!
//! The chart's objects are the Kubernetes table's: the same typed objects, with the images and
//! the inputs as values.

use crate::Format;
use crate::Support::{self, Emitted, Equivalent, NotPortable, Tooling};
use crate::kubernetes::schema::{self, Schema};

const SOURCE: &str = include_str!("../coverage/kubernetes/chart.txt");

/// The pinned Chart.yaml schema's fields.
pub fn schema() -> Schema {
    Schema::parse(SOURCE)
}

/// The definitions with rows of their own: Chart.yaml and its lists' items.
pub const TABLED: &[&str] = &["Chart", "ChartDependencies", "ChartMaintainers"];

/// The section of the chart's files.
pub const FILES: &str = "Chart files";
/// The section of the hooks.
pub const HOOKS: &str = "Hooks";

/// The files a chart may hold, by path within the chart (a directory ends in `/`; `.Files` is
/// any other file the templates read).
pub const CHART_FILES: &[&str] = &[
    "Chart.yaml",
    "values.yaml",
    "values.schema.json",
    "templates/",
    "templates/NOTES.txt",
    "templates/_helpers.tpl",
    "templates/tests/",
    ".Files",
    "charts/",
    "crds/",
    "Chart.lock",
    "LICENSE",
    "README.md",
    ".helmignore",
];

/// The hooks (`helm.sh/hook` values) and the annotations that tune them.
pub const HOOK_FEATURES: &[&str] = &[
    "pre-install",
    "post-install",
    "pre-delete",
    "post-delete",
    "pre-upgrade",
    "post-upgrade",
    "pre-rollback",
    "post-rollback",
    "test",
    "helm.sh/hook-weight",
    "helm.sh/hook-delete-policy",
    "helm.sh/resource-policy",
];

/// The Helm table.
pub fn format() -> Format {
    let mut rows = schema::rows(&schema(), TABLED, chart_yaml);
    rows.extend(CHART_FILES.iter().map(|f| (schema::key(FILES, f), file(f).unwrap_or(Support::Unclassified))));
    rows.extend(HOOK_FEATURES.iter().map(|h| (schema::key(HOOKS, h), hook(h).unwrap_or(Support::Unclassified))));
    Format {
        name: "Helm",
        file: ".isoloom/kubernetes/chart/",
        source: "SchemaStore's chart.json (pinned) and Helm 3.16's chart documentation",
        rows,
        collapse_not_portable: false,
    }
}

const CATALOG: Support = Tooling {
    note: "how the chart is listed in a repository: no effect on the environment",
};
const ONE_CHART: Support = Tooling {
    note: "charts within the chart: an environment is one chart",
};
const PACKAGING: Support = Tooling {
    note: "the chart's packaging and documentation: no effect on the environment",
};

fn chart_yaml(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("Chart", "apiVersion") => Emitted { from: "(v2)" },
        ("Chart", "name") => Emitted { from: "name" },
        ("Chart", "description") => Emitted {
            from: "name (the environment it installs)",
        },
        ("Chart", "type") => Emitted { from: "(application)" },
        ("Chart", "version" | "appVersion") => Emitted {
            from: "(0.1.0; `isoloom chart --version` packages another)",
        },
        ("Chart", "kubeVersion") => Tooling {
            note: "the cluster versions the chart accepts: Isoloom writes objects every supported version has",
        },
        ("Chart", "dependencies") | ("ChartDependencies", _) => ONE_CHART,
        ("Chart", "annotations" | "deprecated" | "home" | "icon" | "keywords" | "maintainers" | "sources") | ("ChartMaintainers", _) => CATALOG,
        _ => return None,
    })
}

fn file(path: &str) -> Option<Support> {
    Some(match path {
        "Chart.yaml" => Emitted { from: "(the chart itself)" },
        "values.yaml" => Emitted {
            from: "machines (each one's image) and inputs",
        },
        "templates/" => Emitted {
            from: "(the environment's objects, its scripts and its inputs)",
        },
        "templates/NOTES.txt" => Emitted {
            from: "checks (how to run them after an install)",
        },
        "templates/tests/" => Emitted {
            from: "checks (a `helm test` Pod per position)",
        },
        ".Files" => Emitted {
            from: "checks and the project's scripts (read into ConfigMaps)",
        },
        "values.schema.json" => Emitted {
            from: "machines (each one's image, required, non-empty) and inputs (scalars): `helm install` rejects any other value",
        },
        "templates/_helpers.tpl" => Tooling {
            note: "named templates shared by a chart's templates: Isoloom writes each object whole",
        },
        "charts/" | "Chart.lock" => ONE_CHART,
        "crds/" => NotPortable {
            why: "custom resource definitions are cluster-wide: no other target has them",
        },
        "LICENSE" | "README.md" | ".helmignore" => PACKAGING,
        _ => return None,
    })
}

fn hook(name: &str) -> Option<Support> {
    Some(match name {
        "test" => Emitted {
            from: "checks (`helm test` runs them)",
        },
        "helm.sh/hook-delete-policy" => Emitted {
            from: "checks (a run replaces the last one's Pods)",
        },
        "post-install" | "post-upgrade" => Equivalent {
            via: "machines.*.docker.init: a container beside the machine's, run once it answers",
        },
        "pre-install" | "pre-upgrade" => Equivalent {
            via: "init containers, which wait for each machine's dependencies",
        },
        "pre-delete" | "post-delete" | "pre-rollback" | "post-rollback" => Tooling {
            note: "steps around Helm's own operations: an environment has no tear-down steps",
        },
        "helm.sh/hook-weight" => Tooling {
            note: "orders hooks of one kind: Isoloom's checks are independent",
        },
        "helm.sh/resource-policy" => Emitted {
            from: "volumes.*.persistent (its claim kept when the release is uninstalled)",
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tabled_definition_is_in_the_schema() {
        let s = schema();
        for d in TABLED {
            assert!(s.fields_of(d).next().is_some(), "{d} has no fields in the pinned schema");
        }
    }
}
