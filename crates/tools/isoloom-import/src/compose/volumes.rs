//! Reading a service's `volumes` entries: what each mounts, and where in the container.

use serde_yaml_ng::Value;

use super::text::str_of;

/// What a volume entry mounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VolumeMount {
    /// A named volume.
    Named(String),
    /// A path from the host (the project folder, most often).
    Bind,
    /// A volume with no name, at a path.
    Anonymous,
}

/// A service's volume entry (`name:/path[:ro]`, `./dir:/path`, `/path`, or the long syntax)
/// and the path in the container.
pub(super) fn volume_mount(item: &Value) -> Option<(VolumeMount, String)> {
    match item {
        Value::String(entry) => {
            let parts: Vec<&str> = entry.split(':').collect();
            match parts.as_slice() {
                [path] => Some((VolumeMount::Anonymous, (*path).to_owned())),
                [source, path, ..] if source.starts_with(['.', '/', '~']) => Some((VolumeMount::Bind, (*path).to_owned())),
                [source, path, ..] => Some((VolumeMount::Named((*source).to_owned()), (*path).to_owned())),
                [] => None,
            }
        }
        Value::Mapping(m) => {
            let target = m.get("target").and_then(str_of)?;
            match (m.get("type").and_then(str_of).as_deref(), m.get("source").and_then(str_of)) {
                (Some("volume"), Some(source)) => Some((VolumeMount::Named(source), target)),
                (Some("volume"), None) => Some((VolumeMount::Anonymous, target)),
                (Some("bind"), Some(_)) => Some((VolumeMount::Bind, target)),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(yaml: &str) -> Result<Option<(VolumeMount, String)>, serde_yaml_ng::Error> {
        Ok(volume_mount(&serde_yaml_ng::from_str(yaml)?))
    }

    #[test]
    fn each_short_form_says_what_it_mounts() -> Result<(), serde_yaml_ng::Error> {
        assert_eq!(
            mount("data:/var/lib/x:ro")?,
            Some((VolumeMount::Named("data".to_owned()), "/var/lib/x".to_owned()))
        );
        assert_eq!(mount("./conf:/etc/x")?, Some((VolumeMount::Bind, "/etc/x".to_owned())));
        assert_eq!(mount("/cache")?, Some((VolumeMount::Anonymous, "/cache".to_owned())));
        Ok(())
    }

    #[test]
    fn the_long_form_says_it_by_type() -> Result<(), serde_yaml_ng::Error> {
        assert_eq!(
            mount("{ type: volume, source: shared, target: /in }")?,
            Some((VolumeMount::Named("shared".to_owned()), "/in".to_owned()))
        );
        assert_eq!(mount("{ type: tmpfs, target: /run }")?, None);
        Ok(())
    }
}
