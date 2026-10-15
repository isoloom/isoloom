//! Each Linux OS's official container image, pinned by digest: the container form of a machine
//! that is its OS alone (kept running idle), and the base of the Dockerfile Isoloom generates
//! from a machine's portable steps. A digest is the image's multi-architecture index, so `amd64`
//! and `arm64` machines pull the same reference. Windows has none in 1.0.
//!
//! Refreshed by hand: `docker buildx imagetools inspect <image> --format '{{json .Manifest.Digest}}'`.

/// Image-table name → the official image, by tag and digest.
const IMAGES: &[(&str, &str)] = &[
    ("debian-11", "debian:11@sha256:6f519a81440354a85eb592c5f32109ab80605f6b892455983a6f618bf87fabe9"),
    ("debian-12", "debian:12@sha256:2c037a04925515fdd6ea85ea14a682d0e79931f5e9f5d07b6dbfc6ba12f9e858"),
    ("debian-13", "debian:13@sha256:913f6706df59a68922d1dd08f78c2476560a8d367897200a6005b00e5f67c2d5"),
    (
        "ubuntu-14.04",
        "ubuntu:14.04@sha256:64483f3496c1373bfd55348e88694d1c4d0c9b660dee6bfef5e12f43b9933b30",
    ),
    (
        "ubuntu-16.04",
        "ubuntu:16.04@sha256:1f1a2d56de1d604801a9671f301190704c25d604a416f59e03c04f5c6ffee0d6",
    ),
    (
        "ubuntu-18.04",
        "ubuntu:18.04@sha256:152dc042452c496007f07ca9127571cb9c29697f42acbfad72324b2bb2e43c98",
    ),
    (
        "ubuntu-20.04",
        "ubuntu:20.04@sha256:8feb4d8ca5354def3d8fce243717141ce31e2c428701f6682bd2fafe15388214",
    ),
    (
        "ubuntu-22.04",
        "ubuntu:22.04@sha256:5ec03bb3441e8b0bf3b4f9cd4629a1ae763010dc3035bb8da3ae6cf026486401",
    ),
    (
        "ubuntu-24.04",
        "ubuntu:24.04@sha256:534baea6a22c03a63003dbc8dbe78fe34bc0d7e595d9a9dc9834884ff530eb55",
    ),
    (
        "rocky-9",
        "rockylinux/rockylinux:9@sha256:8101994123cf3d0a8fee517bee7f39e555c7d92bd2d9eb3303cc988a0eeed00f",
    ),
    (
        "almalinux-9",
        "almalinux:9@sha256:9819dc675b67b595c2b59e42be7763fca1a8bb217fa5944e04daa22e9a64db16",
    ),
    (
        "alpine-3.19",
        "alpine:3.19@sha256:6baf43584bcb78f2e5847d1de515f23499913ac9f12bdf834811a3145eb11ca1",
    ),
    (
        "kali",
        "kalilinux/kali-rolling@sha256:02f5d6c019e549b0f55da58e835cad5355ef1030c6b2645265a324e37dfc4acc",
    ),
    ("fedora-42", "fedora:42@sha256:99e203b80b1c3d8f7e161ec10a68fd02b081ef83a3963553e513c82846b97814"),
    ("centos-7", "centos:7@sha256:be65f488b7764ad3638f236b7b515b3678369a5124c47b8d32916d6487418ea4"),
];

/// The official container image of the OS named `os` in the image table (`debian-12`), pinned
/// by digest; `None` for Windows.
pub fn container_image(os: &str) -> Option<&'static str> {
    IMAGES.iter().find(|(n, _)| *n == os).map(|(_, image)| *image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::KNOWN_OS;

    #[test]
    fn every_linux_os_has_a_pinned_image_and_windows_none() {
        for os in KNOWN_OS {
            match container_image(os) {
                Some(image) => assert!(image.contains("@sha256:") && image.len() > 71, "{os}: {image}"),
                None => assert!(crate::images::is_windows(os), "{os} has no image"),
            }
        }
    }
}
