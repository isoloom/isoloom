//! Fixtures for the storage no committed example has: volumes kept past `down` or existing
//! already, a folder of a shared volume, sized tmpfs mounts, and data disks.

/// Containers on Docker: a shared volume one machine takes a folder of, a persistent and an
/// existing volume, a tmpfs with a size and a mode, a data disk.
#[allow(dead_code)] // each test file uses the fixtures of its formats.
pub const CONTAINERS: &str = r#"
version: 1
name: storage-fixture
networks: { lan: { cidr: 10.9.0.0/24 } }
volumes: { site: { owner: web }, db: { persistent: true }, cache: { external: shared-cache } }
machines:
  web:
    networks: { lan: 10 }
    docker: { image: "nginx:alpine" }
    volumes: { site: /usr/share/nginx/html, cache: /cache }
    tmpfs: [/tmp, { path: /run/app, size: 16m, mode: "1777" }]
    disks: { data: { size_gb: 1, mount: /data } }
  app:
    networks: { lan: 11 }
    docker: { image: "alpine:3.20", idle: true }
    volumes: { site: { path: /srv, subpath: www }, db: /var/lib/db }
"#;

/// The same on Kubernetes, which has no tmpfs mode, with a raw disk (a block device).
#[allow(dead_code)] // each test file uses the fixtures of its formats.
pub const KUBERNETES: &str = r#"
version: 1
name: storage-fixture
networks: { lan: { cidr: 10.9.0.0/24 } }
volumes: { site: { owner: web }, db: { persistent: true }, cache: { external: shared-cache } }
machines:
  web:
    networks: { lan: 10 }
    docker: { image: "nginx:alpine" }
    volumes: { site: /usr/share/nginx/html, cache: /cache }
    tmpfs: [{ path: /run/app, size: 16m }]
    disks: { data: { size_gb: 1, mount: /data } }
  app:
    networks: { lan: 11 }
    docker: { image: "alpine:3.20", idle: true }
    volumes: { site: { path: /srv, subpath: www }, db: /var/lib/db }
    disks: { blk: { size_gb: 1, fs: raw } }
"#;

/// A VM with a formatted and a raw data disk.
#[allow(dead_code)] // each test file uses the fixtures of its formats.
pub const DISKS: &str = r"
version: 1
name: disks-fixture
networks: { lan: { cidr: 10.9.0.0/24 } }
machines:
  db:
    networks: { lan: 10 }
    vm: { os: debian-12, provision: [setup.sh] }
    disks: { data: { size_gb: 2, fs: xfs, mount: /data }, blk: { size_gb: 1, fs: raw } }
";
