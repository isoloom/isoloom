//! What a drawn container is in each form. Isoloom renders a spec to true containers or true VMs,
//! never containers inside a VM, so a container node is written as a machine with both forms
//! wherever Isoloom has a VM form of its image:
//!
//! - a distro image (`debian:12`, `alpine:3.24`) is its OS (`os: debian@12`): the OS's image as a
//!   container, a fresh install as a VM ([`matrix::of`], the table a translated Dockerfile's `FROM`
//!   reads);
//! - a service image the catalogue has (`nginx:1.30-alpine`, `postgres:16`) is that catalogue
//!   service at the image's version (`os` + `service: nginx@1.30`), on the OS its tag names when
//!   the release runs there, else the release's first; the entry's recipe gives both forms. It
//!   is used only when the catalogue resolves it with no settings and its ports hold every port
//!   drawn on the node, so the drawn machine answers where it was drawn to.
//!
//! Any other image stays an image (`app.image`): a container only, which a VM target refuses,
//! naming the machine and why.

use isoloom_catalogue::{Catalogue, Entry, Version, pick};
use isoloom_plan::dockerfile::matrix;
use isoloom_spec::syntax::{self, OsRef};

use crate::Service;

/// The words of an image's tag that name no version (`nginx:latest` is the newest release).
const NO_VERSION: [&str; 5] = ["latest", "lts", "current", "slim", "alpine"];

/// What a drawn container is written as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Forms {
    /// Its image: a container only.
    Image,
    /// Its OS alone: both forms.
    Os(OsRef),
    /// A catalogue service on an OS: both forms, from the entry's recipe.
    Service {
        /// The OS it runs on.
        os: OsRef,
        /// The service, as `service:` writes it (`nginx@1.30`).
        service: String,
    },
}

/// The forms of a container drawn from `image` and answering on `ports`.
pub(crate) fn of(image: &str, ports: &[Service]) -> Forms {
    let (name, tag) = matrix::split(image);
    let (entry, version) = match matrix::of(image) {
        Ok(base) if base.entry.is_none() && base.prelude.is_empty() => {
            return base.os.parse().map_or(Forms::Image, Forms::Os);
        }
        Ok(base) => match base.entry {
            Some(call) => (call.entry.to_owned(), call.version),
            None => return Forms::Image,
        },
        Err(_) => (name, tag_version(&tag)),
    };
    Catalogue::embedded()
        .ok()
        .and_then(|catalogue| service(catalogue, &entry, version.as_deref(), &tag, ports))
        .unwrap_or(Forms::Image)
}

/// The version a tag names (`1.30` of `1.30-alpine`), or none for the newest.
fn tag_version(tag: &str) -> Option<String> {
    let version = tag.split('-').next().unwrap_or(tag);
    (!NO_VERSION.contains(&version)).then(|| version.to_owned())
}

/// The catalogue's `entry` at `version` (none: its default) as a service machine, when it
/// resolves with no settings and its ports hold every drawn port.
fn service(catalogue: &Catalogue, entry: &str, version: Option<&str>, tag: &str, ports: &[Service]) -> Option<Forms> {
    let found = catalogue.entry(entry)?;
    let asked: Version = match version {
        Some(v) => v.parse().ok()?,
        None => found.definition().default.clone(),
    };
    let release = pick(found.definition().releases.iter().map(|r| &r.version), &asked)?;
    let os = release_os(found, release, tag)?;
    let service = match version {
        Some(v) => format!("{entry}@{v}"),
        None => entry.to_owned(),
    };
    let probe = format!("version: 1\nname: probe\nmachines:\n  m:\n    os: {os}\n    service: {service}\n");
    let spec = syntax::parse(&probe).ok()?;
    let resolved = catalogue.resolve(&spec, "m").ok()?;
    let answers = |s: &Service| resolved.iter().flat_map(|r| r.ports.values()).any(|p| p.port == s.port);
    ports.iter().all(answers).then_some(Forms::Service { os, service })
}

/// The OS release `release` of `entry` runs on: the one the image's tag names (`-alpine`), else
/// the first it lists.
fn release_os(entry: &Entry, release: &Version, tag: &str) -> Option<OsRef> {
    let oses = &entry.definition().releases.iter().find(|r| &r.version == release)?.os;
    let named = tag.split('-').skip(1).find_map(|word| oses.iter().find(|os| word.starts_with(os.os().family)));
    named.or_else(|| oses.first()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(port: u16) -> Service {
        serde_json::from_str(&format!(r#"{{ "port": {port}, "protocol": "http" }}"#)).unwrap()
    }

    #[test]
    fn a_distro_image_is_its_os_and_one_isoloom_has_no_vm_of_stays_an_image() {
        assert_eq!(of("debian:12", &[]), Forms::Os("debian@12".parse().unwrap()));
        assert_eq!(of("alpine:3.24", &[]), Forms::Os("alpine@3.24".parse().unwrap()));
        assert_eq!(of("alpine:3.20", &[]), Forms::Image);
    }

    #[test]
    fn a_catalogue_image_is_its_service_on_the_os_its_tag_names() {
        let nginx = of("nginx:1.30-alpine", &[http(80)]);
        assert_eq!(
            nginx,
            Forms::Service {
                os: "alpine@3.24".parse().unwrap(),
                service: "nginx@1.30".to_owned()
            }
        );
        assert!(matches!(of("nginx", &[]), Forms::Service { service, .. } if service == "nginx"));
    }

    #[test]
    fn an_image_stays_one_when_the_catalogue_lacks_its_version_or_a_drawn_port() {
        assert_eq!(of("nginx:1.27-alpine", &[]), Forms::Image, "no 1.27 release");
        assert_eq!(of("nginx:1.30", &[http(8080)]), Forms::Image, "nginx answers on 80");
        assert_eq!(of("frrouting/frr:v8.5.2", &[]), Forms::Image);
        assert_eq!(of("redis:7-alpine", &[]), Forms::Image);
    }
}
