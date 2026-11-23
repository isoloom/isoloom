//! Reading a vendor repository's index for one package's versions: apt's `Packages`, the
//! `primary` metadata of an rpm repository (found through its `repomd.xml`), and Alpine's
//! `APKINDEX`. Each version is the string its package manager installs (`1.30.6-1~bookworm`,
//! `1.30.6-1.el9.ngx`, `1.30.6-r1`); [`upstream`] says which upstream release it is.

use isoloom_catalogue::Family;

/// The versions of package `name` in an apt `Packages` file.
pub(crate) fn apt(packages: &str, name: &str) -> Vec<String> {
    packages
        .split("\n\n")
        .filter(|stanza| field(stanza, "Package") == Some(name))
        .filter_map(|stanza| field(stanza, "Version").map(str::to_owned))
        .collect()
}

/// A stanza's field.
fn field<'a>(stanza: &'a str, key: &str) -> Option<&'a str> {
    stanza.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(':')).map(str::trim)
}

/// The primary metadata's path, from an rpm repository's `repomd.xml`.
pub(crate) fn rpm_primary(repomd: &str) -> Option<&str> {
    let data = repomd.split("<data ").find(|d| d.starts_with("type=\"primary\""))?;
    attribute(data.split("<location").nth(1)?, "href")
}

/// The versions (`<ver>-<rel>`) of package `name` for x86-64 in an rpm repository's primary
/// metadata.
pub(crate) fn rpm(primary: &str, name: &str) -> Vec<String> {
    primary
        .split("<package ")
        .skip(1)
        .filter(|p| element(p, "name") == Some(name))
        .filter(|p| matches!(element(p, "arch"), Some("x86_64" | "noarch")))
        .filter_map(|p| {
            let version = p.split("<version ").nth(1)?;
            Some(format!("{}-{}", attribute(version, "ver")?, attribute(version, "rel")?))
        })
        .collect()
}

/// The text of element `tag` in `xml`.
fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{tag}>"))? + tag.len() + 2;
    let rest = xml.get(start..)?;
    rest.get(..rest.find('<')?)
}

/// The value of attribute `name` in the start of an element.
fn attribute<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let start = xml.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = xml.get(start..)?;
    rest.get(..rest.find('"')?)
}

/// The versions of package `name` in an Alpine `APKINDEX`.
pub(crate) fn apk(index: &str, name: &str) -> Vec<String> {
    index
        .split("\n\n")
        .filter(|block| block.lines().any(|l| l.strip_prefix("P:") == Some(name)))
        .filter_map(|block| block.lines().find_map(|l| l.strip_prefix("V:")).map(str::to_owned))
        .collect()
}

/// The upstream release a package version of `family` is (`1.30.6` for `1:1.30.6-1~bookworm`,
/// `1.30.6-1.el9.ngx` and `1.30.6-r1`).
pub(crate) fn upstream(version: &str, family: Family) -> &str {
    match family {
        Family::RedHat => version.split_once('-').map_or(version, |(v, _)| v),
        Family::Alpine => version.rsplit_once("-r").map_or(version, |(v, _)| v),
        _ => {
            let no_epoch = version.split_once(':').map_or(version, |(_, v)| v);
            no_epoch.rsplit_once('-').map_or(no_epoch, |(v, _)| v)
        }
    }
}

/// The newest of `versions`, digits compared as numbers.
pub(crate) fn newest(versions: &[String]) -> Option<&String> {
    versions.iter().max_by(|a, b| natural(a).cmp(&natural(b)))
}

/// `s` split into runs of digits (as numbers) and other text, for ordering.
fn natural(s: &str) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let mut other = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !other.is_empty() {
                out.push((0, std::mem::take(&mut other)));
            }
            digits.push(c);
        } else {
            if !digits.is_empty() {
                out.push((digits.parse().unwrap_or(u64::MAX), String::new()));
                digits.clear();
            }
            other.push(c);
        }
    }
    if !digits.is_empty() {
        out.push((digits.parse().unwrap_or(u64::MAX), String::new()));
    }
    if !other.is_empty() {
        out.push((0, other));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_lists_a_packages_versions() {
        let packages = "Package: nginx\nVersion: 1.30.5-1~bookworm\nArchitecture: amd64\n\nPackage: nginx-dbg\nVersion: 1.30.6-1~bookworm\n\nPackage: nginx\nVersion: 1.30.6-1~bookworm\n";
        assert_eq!(apt(packages, "nginx"), ["1.30.5-1~bookworm", "1.30.6-1~bookworm"]);
    }

    #[test]
    fn rpm_finds_the_primary_metadata_then_the_versions() {
        let repomd = r#"<repomd><data type="other"><location href="repodata/o.xml.gz"/></data><data type="primary"><checksum>x</checksum><location href="repodata/abc-primary.xml.gz"/></data></repomd>"#;
        assert_eq!(rpm_primary(repomd), Some("repodata/abc-primary.xml.gz"));
        let primary = r#"<metadata><package type="rpm"><name>nginx</name><arch>x86_64</arch><version epoch="1" ver="1.30.6" rel="1.el9.ngx"/></package><package type="rpm"><name>nginx</name><arch>aarch64</arch><version epoch="1" ver="1.30.7" rel="1.el9.ngx"/></package><package type="rpm"><name>nginx-module-njs</name><arch>x86_64</arch><version ver="1.30.6" rel="1"/></package></metadata>"#;
        assert_eq!(rpm(primary, "nginx"), ["1.30.6-1.el9.ngx"]);
    }

    #[test]
    fn apk_lists_a_packages_versions() {
        let index = "C:Q1x\nP:nginx\nV:1.30.6-r1\nA:x86_64\n\nC:Q1y\nP:nginx-module-geoip\nV:1.30.6-r1\n\nP:nginx\nV:1.30.5-r1\n";
        assert_eq!(apk(index, "nginx"), ["1.30.6-r1", "1.30.5-r1"]);
    }

    #[test]
    fn each_family_names_its_upstream_release() {
        assert_eq!(upstream("1:1.30.6-1~bookworm", Family::Debian), "1.30.6");
        assert_eq!(upstream("1.30.6-1~noble", Family::Debian), "1.30.6");
        assert_eq!(upstream("1.30.6-1.el9.ngx", Family::RedHat), "1.30.6");
        assert_eq!(upstream("1.30.6-r1", Family::Alpine), "1.30.6");
    }

    #[test]
    fn the_newest_compares_numbers_as_numbers() {
        let v = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(newest(&v(&["1.30.6-r9", "1.30.6-r10"])).map(String::as_str), Some("1.30.6-r10"));
        assert_eq!(
            newest(&v(&["1.30.6-1~bookworm", "1.30.6-2~bookworm"])).map(String::as_str),
            Some("1.30.6-2~bookworm")
        );
    }
}
