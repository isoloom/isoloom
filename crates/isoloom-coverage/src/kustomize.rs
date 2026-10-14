//! Kustomize: the fields of `kustomization.yaml`, from SchemaStore's pinned schema
//! (`coverage/kubernetes/kustomization.txt`, see `scripts/refresh-kubernetes.py`). The
//! kustomization's own fields have rows, and so does every definition Isoloom writes into (its
//! ConfigMap generator and its options); the tests fail when it writes into another one.
//!
//! Kustomize is how the manifests are applied, with the project's scripts beside them: almost
//! everything else it offers edits objects Isoloom already writes as they should be.

use crate::Format;
use crate::Support::{self, Emitted, Equivalent, Tooling};
use crate::kubernetes::schema::{self, Schema};

const SOURCE: &str = include_str!("../coverage/kubernetes/kustomization.txt");

/// The pinned kustomization schema's fields.
pub fn schema() -> Schema {
    Schema::parse(SOURCE)
}

/// The definitions with rows of their own: the kustomization, then the ones Isoloom writes into.
pub const TABLED: &[&str] = &["Kustomization", "ConfigMapArgs", "GeneratorOptions"];

/// The Kustomize table.
pub fn format() -> Format {
    Format {
        name: "Kustomize",
        file: ".isoloom/kubernetes/kustomization.yaml",
        source: "SchemaStore's kustomization.json (pinned)",
        rows: schema::rows(&schema(), TABLED, classify),
        collapse_not_portable: false,
    }
}

const EDITS: Support = Tooling {
    note: "edits the objects it builds: Isoloom writes them as they should be",
};
const PLUGINS: Support = Tooling {
    note: "Kustomize's plugins and build settings: no effect on the environment",
};
const SCRIPTS: Support = Emitted {
    from: "(the project's scripts the pods run: `init:` jobs, checks)",
};

fn classify(definition: &str, field: &str) -> Option<Support> {
    Some(match (definition, field) {
        ("Kustomization", "apiVersion" | "kind" | "resources") => Emitted {
            from: "(the manifests, and the checks' Jobs beside them)",
        },
        ("Kustomization", "namespace") => Emitted {
            from: "(the environment's namespace)",
        },
        ("Kustomization", "configMapGenerator" | "generatorOptions") | ("ConfigMapArgs", "name" | "files") => SCRIPTS,
        ("GeneratorOptions", "disableNameSuffixHash") => Emitted {
            from: "(the pods name the scripts' ConfigMap as it is)",
        },
        ("Kustomization", "secretGenerator") => Equivalent {
            via: "the chart's inputs Secret, from its values",
        },
        ("Kustomization", "images") => Equivalent {
            via: "the chart's `images` values",
        },
        ("Kustomization", "helmCharts" | "helmGlobals") => Tooling {
            note: "Helm inside Kustomize: Isoloom writes the chart beside the manifests",
        },
        ("Kustomization", "crds") => Tooling {
            note: "custom resource definitions: Isoloom writes built-in kinds only",
        },
        ("Kustomization", "bases" | "components") => Tooling {
            note: "kustomizations layered on others: Isoloom's is one layer",
        },
        (
            "Kustomization",
            "commonAnnotations"
            | "commonLabels"
            | "labels"
            | "namePrefix"
            | "nameSuffix"
            | "patches"
            | "patchesJson6902"
            | "patchesStrategicMerge"
            | "replacements"
            | "replicas"
            | "vars"
            | "sortOptions",
        )
        | ("ConfigMapArgs", "behavior" | "namespace" | "options")
        | ("GeneratorOptions", "annotations" | "immutable" | "labels") => EDITS,
        ("Kustomization", "buildMetadata" | "configurations" | "generators" | "inventory" | "metadata" | "openapi" | "transformers" | "validators") => PLUGINS,
        ("ConfigMapArgs", "KVSources" | "env" | "envs" | "literals") => Equivalent {
            via: "`files`: each script as it is",
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
