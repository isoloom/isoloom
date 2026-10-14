//! A machine's `files` on Kubernetes. A ConfigMap's files are owned by root with one mode for
//! the volume, so the pod's `files` init container (Isoloom's busybox) copies each into an
//! emptyDir and gives it its owner and mode; the machine's containers mount each file of the
//! emptyDir at its path (`subPath`), leaving the rest of the folder as the image has it.
//!
//! The text written in the spec is the machine's own ConfigMap (`<machine>-files`, key
//! `file-<n>`); a project file comes from the project's ConfigMap (the scripts', read from the
//! project at apply time, copied into the chart).

use indexmap::IndexMap;
use isoloom_compose::docker::PROBE_IMAGE;
use isoloom_spec::shell;

use crate::kustomize::key;
use crate::machine::Machine;
use crate::objects::{Container, EmptyDir, Image, Manifest, Text, Volume, VolumeMount, VolumeSource};

/// The emptyDir holding the files as placed, and the init container filling it.
pub const FILES_VOLUME: &str = "isoloom-files";
/// The volume of the machine's own ConfigMap (the text written in the spec).
const TEXT_VOLUME: &str = "isoloom-files-text";
/// Where the init container sees the emptyDir and the ConfigMaps.
const DIR: &str = "/isoloom-files";

/// The machine's ConfigMap of the text written in the spec.
fn config_map_name(m: &Machine) -> String {
    format!("{}-files", m.name)
}

/// The machine's ConfigMap of the text its files carry, when one does.
pub(crate) fn config_map(m: &Machine) -> Option<Manifest> {
    let data: IndexMap<String, Text> = m
        .plan
        .files
        .iter()
        .enumerate()
        .filter_map(|(n, f)| Some((format!("file-{n}"), Text::Literal(f.content.clone()?))))
        .collect();
    (!data.is_empty()).then(|| Manifest::text_config_map(&config_map_name(m), data))
}

/// The project files the machine's files copy, from the project folder.
pub(crate) fn sources(m: &Machine) -> Vec<String> {
    m.plan.files.iter().filter_map(|f| f.source.clone()).collect()
}

/// Whether its files read the project's ConfigMap.
pub(crate) fn reads_project(m: &Machine) -> bool {
    m.plan.files.iter().any(|f| f.source.is_some())
}

/// Where its containers have the files: each at its path, from the emptyDir.
pub(crate) fn mounts(m: &Machine) -> Vec<VolumeMount> {
    m.plan
        .files
        .iter()
        .enumerate()
        .map(|(n, f)| VolumeMount {
            sub_path: Some(n.to_string()),
            ..VolumeMount::read_only(FILES_VOLUME, &f.path)
        })
        .collect()
}

/// The init container placing the files in the emptyDir, each with its owner and mode, as
/// root; `None` when the machine has no files.
pub(crate) fn init_container(m: &Machine, project: &str) -> Option<Container> {
    if m.plan.files.is_empty() {
        return None;
    }
    let script: Vec<String> = m
        .plan
        .files
        .iter()
        .enumerate()
        .map(|(n, f)| {
            let from = match &f.source {
                Some(source) => format!("{DIR}/project/{}", key(source)),
                None => format!("{DIR}/text/file-{n}"),
            };
            let to = format!("{DIR}/out/{n}");
            format!(
                "cp {} {to} && chown {}:{} {to} && chmod {} {to}",
                shell::quote(&from),
                f.uid.unwrap_or(0),
                f.gid.unwrap_or(0),
                f.mode_or_default()
            )
        })
        .collect();
    let mut c = Container::new(FILES_VOLUME, Image::Tool(m.env.images.pull(PROBE_IMAGE)));
    c.command = vec!["sh".into(), "-c".into(), script.join(" && ")];
    c.volume_mounts = vec![VolumeMount::at(FILES_VOLUME, format!("{DIR}/out"))];
    if config_map(m).is_some() {
        c.volume_mounts.push(VolumeMount::read_only(TEXT_VOLUME, format!("{DIR}/text")));
    }
    if reads_project(m) {
        c.volume_mounts.push(VolumeMount::read_only(project, format!("{DIR}/project")));
    }
    Some(c)
}

/// The pod's volumes for the files: the emptyDir, and the machine's ConfigMap when it has one
/// (the project's is the pod's scripts volume).
pub(crate) fn volumes(m: &Machine) -> Vec<Volume> {
    if m.plan.files.is_empty() {
        return Vec::new();
    }
    let mut out = vec![Volume {
        name: FILES_VOLUME.into(),
        source: VolumeSource::EmptyDir(EmptyDir::default()),
    }];
    if config_map(m).is_some() {
        out.push(Volume {
            name: TEXT_VOLUME.into(),
            source: crate::pod::config_map_volume(&config_map_name(m)),
        });
    }
    out
}
