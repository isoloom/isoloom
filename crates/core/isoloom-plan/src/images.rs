//! Which image a machine pulls: the spec's `constraints.registries` send each pull through its
//! registry's mirror, decided here once so a backend writes the final reference instead of
//! patching its output afterwards.

use isoloom_spec::model::{Constraints, Spec};

/// `image` pulled through the mirror of its registry, when there is one.
pub fn rewrite_image(c: &Constraints, image: &str) -> String {
    if c.registries.is_empty() {
        return image.to_string();
    }
    let (first, rest) = match image.split_once('/') {
        Some((f, r)) if f.contains('.') || f.contains(':') || f == "localhost" => (f, r.to_string()),
        _ => ("docker.io", if image.contains('/') { image.to_string() } else { format!("library/{image}") }),
    };
    let registry = match first {
        "index.docker.io" | "registry-1.docker.io" => "docker.io",
        r => r,
    };
    match c.registries.get(registry) {
        Some(mirror) => format!("{}/{rest}", mirror.trim_end_matches('/')),
        None => image.to_string(),
    }
}

/// The tag a machine's container image goes by: the published one, or the name its build is
/// tagged with (so its `init:` jobs can reuse it).
pub fn tag_of(spec: &Spec, name: &str, image: Option<&str>) -> String {
    image.map_or_else(|| format!("isoloom/{}-{name}", spec.name), str::to_string)
}

/// Resolves the image references of one environment: pulls through the mirrors, except the
/// images the project builds itself (tagged locally, never pulled).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Images {
    constraints: Option<Constraints>,
    built: Vec<String>,
}

impl Images {
    /// The resolver of `spec`'s images.
    pub fn of(spec: &Spec) -> Images {
        let built = spec
            .machines
            .iter()
            .filter_map(|(name, m)| {
                let d = m.docker.as_ref()?;
                (d.build.is_some() || d.generated.is_some()).then(|| tag_of(spec, name, d.image.as_deref()))
            })
            .collect();
        let constraints = spec.constraints.clone().filter(|c| !c.registries.is_empty());
        Images { constraints, built }
    }

    /// The reference to write for `image`.
    pub fn pull(&self, image: &str) -> String {
        match &self.constraints {
            Some(c) if !self.built.iter().any(|b| b == image) => rewrite_image(c, image),
            _ => image.to_string(),
        }
    }
}
