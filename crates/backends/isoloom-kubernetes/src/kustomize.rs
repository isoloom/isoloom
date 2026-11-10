//! The kustomizations: the environment's (its manifests, and the project's scripts as a
//! `ConfigMap` read from the chart's copies, inside the kustomization's folder) and the checks'
//! (their runners). Typed, so the command line reads back which project files to copy
//! ([`project_scripts`]).

use serde::{Deserialize, Serialize};

use crate::layout::PROJECT_COPIES;
use crate::objects::Labels;

/// A `kustomization.yaml`, the fields Isoloom writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Kustomization {
    /// `kustomize.config.k8s.io/v1beta1`.
    pub(crate) api_version: String,
    /// `Kustomization`.
    pub(crate) kind: String,
    /// The namespace every object goes in.
    pub(crate) namespace: String,
    /// The manifests, next to it.
    pub(crate) resources: Vec<String>,
    /// How generated `ConfigMap`s are named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) generator_options: Option<GeneratorOptions>,
    /// `ConfigMap`s made from files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) config_map_generator: Vec<ConfigMapGenerator>,
}

/// How generated `ConfigMap`s are named and labelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratorOptions {
    /// Their names as written (the pods name them), without kustomize's hash.
    pub(crate) disable_name_suffix_hash: bool,
    /// Their labels, every other object's ([`crate::objects::common_labels`]).
    #[serde(default, skip_serializing_if = "Labels::is_empty")]
    pub(crate) labels: Labels,
}

/// A `ConfigMap` made from files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigMapGenerator {
    /// The `ConfigMap`.
    pub(crate) name: String,
    /// Its files: `<key>=<path>`, or a path whose file name is the key.
    pub(crate) files: Vec<String>,
}

/// The `ConfigMap` of the project's scripts the pods run (init jobs, checks).
pub(crate) const SCRIPTS: &str = "isoloom-scripts";

/// The `ConfigMap` of the check runners.
pub(crate) const CHECK_SCRIPTS: &str = "isoloom-check-runners";

impl Kustomization {
    /// The objects in `resources`, in `namespace`, plus a `ConfigMap` `name` of `files`, labelled
    /// `labels`, when there are any.
    pub(crate) fn new(namespace: &str, resource: &str, name: &str, files: Vec<String>, labels: &Labels) -> Kustomization {
        let generates = !files.is_empty();
        Kustomization {
            api_version: "kustomize.config.k8s.io/v1beta1".into(),
            kind: "Kustomization".into(),
            namespace: namespace.into(),
            resources: vec![resource.into()],
            generator_options: generates.then(|| GeneratorOptions {
                disable_name_suffix_hash: true,
                labels: labels.clone(),
            }),
            config_map_generator: if generates {
                vec![ConfigMapGenerator { name: name.into(), files }]
            } else {
                Vec::new()
            },
        }
    }
}

/// The longest `ConfigMap` key.
const KEY_MAX: usize = 253;

/// A `ConfigMap` key for a project file, one per path: keys allow only letters, digits, `-`, `_`
/// and `.`, never start with `..` and are at most [`KEY_MAX`] characters, so a `/` is written `__`
/// and any other byte, `_` included, `_` and its two hex digits (`my seed.sh` is
/// `my_20seed.sh`), a leading `.` too. Every escape starts with `_`, so two paths never share a
/// key, and the key is a single word a shell command can name unquoted. A key that would be too
/// long keeps its start and ends with `_` and the path's hash.
pub(crate) fn key(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'/' => out.push_str("__"),
            b'.' if out.is_empty() => out.push_str("_2e"),
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' => out.push(char::from(b)),
            other => {
                out.push('_');
                out.extend([other >> 4, other & 0xf].into_iter().filter_map(|d| char::from_digit(d.into(), 16)));
            }
        }
    }
    if out.len() > KEY_MAX {
        let hash = format!("_{:08x}", isoloom_render::stable_hash::of(path));
        out.truncate(KEY_MAX - hash.len());
        out.push_str(&hash);
    }
    out
}

/// The environment's kustomization: its manifests, and the project's `scripts` (paths from the
/// project folder) as the [`SCRIPTS`] `ConfigMap`, labelled `labels`, read from their copies in
/// [`PROJECT_COPIES`].
pub(crate) fn environment(namespace: &str, scripts: &[String], labels: &Labels) -> Kustomization {
    let files = scripts.iter().map(|f| format!("{}={PROJECT_COPIES}/{f}", key(f))).collect();
    Kustomization::new(namespace, "environment.yaml", SCRIPTS, files, labels)
}

/// The project's scripts an environment's kustomization reads, as paths from the project folder.
pub(crate) fn project_scripts(kustomization: &str) -> Result<Vec<String>, serde_yaml_ng::Error> {
    let k: Kustomization = serde_yaml_ng::from_str(kustomization)?;
    let files = k.config_map_generator.into_iter().filter(|g| g.name == SCRIPTS).flat_map(|g| g.files);
    Ok(files
        .filter_map(|f| {
            let (_, copy) = f.split_once('=')?;
            Some(copy.strip_prefix(PROJECT_COPIES)?.strip_prefix('/')?.to_owned())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::layout::{CHART, CHART_PROJECT_FILES};

    #[test]
    fn the_scripts_read_back_are_those_written() {
        let scripts = vec!["build/cache/seed.sh".to_owned(), "checks/up.sh".to_owned()];
        let text = serde_yaml_ng::to_string(&environment("isoloom-x", &scripts, &Labels::new())).unwrap();
        assert_eq!(project_scripts(&text).unwrap(), scripts);
    }

    #[test]
    fn the_scripts_are_read_from_inside_the_kustomization_folder() {
        let k = environment("isoloom-x", &["build/cache/seed.sh".to_owned()], &Labels::new());
        assert_eq!(
            k.config_map_generator[0].files,
            ["build__cache__seed.sh=chart/files/project/build/cache/seed.sh"]
        );
        assert_eq!(PROJECT_COPIES, format!("{CHART}/{CHART_PROJECT_FILES}"), "where the chart has them");
    }

    #[test]
    fn no_scripts_no_config_map() {
        let k = environment("isoloom-x", &[], &Labels::new());
        assert!(k.generator_options.is_none() && k.config_map_generator.is_empty());
        let text = serde_yaml_ng::to_string(&k).unwrap();
        assert_eq!(project_scripts(&text).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_key_is_a_configmap_key_and_one_per_path() {
        assert_eq!(key("build/cache/seed.sh"), "build__cache__seed.sh");
        assert_eq!(key("build/cache/my seed.sh"), "build__cache__my_20seed.sh");
        assert_ne!(key("a/b__c"), key("a__b/c"));
        assert_eq!(key("a_b"), "a_5fb");
    }

    #[test]
    fn a_key_never_starts_with_a_dot_nor_outgrows_kubernetes() {
        assert_eq!(key("..config.sh"), "_2e.config.sh");
        assert_eq!(key(".env.sh"), "_2eenv.sh");
        assert_ne!(key("..a"), key("_2e.a"));
        let long = format!("{}/{}.sh", "d".repeat(200), "f".repeat(200));
        let (k, other) = (key(&long), key(&long.replace(".sh", ".bash")));
        assert_eq!(k.len(), KEY_MAX);
        assert_ne!(k, other, "two long paths keep a key each");
    }

    proptest! {
        #[test]
        fn two_paths_never_share_a_key(a in "[a-z_/ é.-]{1,8}", b in "[a-z_/ é.-]{1,8}") {
            prop_assume!(a != b);
            prop_assert_ne!(key(&a), key(&b));
            prop_assert!(key(&a).chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)), "{}", key(&a));
            prop_assert!(!key(&a).starts_with('.'), "{}", key(&a));
        }

        #[test]
        fn any_scripts_read_back_are_those_written(scripts in prop::collection::vec("[a-z0-9_.-]{1,8}(/[a-z0-9_.-]{1,8}){0,2}", 0..4)) {
            let text = serde_yaml_ng::to_string(&environment("isoloom-x", &scripts, &Labels::new())).expect("writes");
            prop_assert_eq!(project_scripts(&text).expect("reads"), scripts);
        }
    }
}
