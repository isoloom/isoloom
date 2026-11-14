//! What a well-known port installs on each Linux family, and the lines that make it answer on the
//! project's networks and start where an init system runs it.

use isoloom_spec::os::{Installer, Os};

/// How an OS installs packages and runs services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Family {
    /// Debian, Ubuntu, Kali: `apt-get`, systemd.
    Apt,
    /// Rocky, Alma, Fedora: `dnf`, systemd.
    Dnf,
    /// CentOS 7: `yum`, systemd.
    Yum,
    /// Alpine: `apk`, OpenRC.
    Apk,
}

impl Family {
    /// How `os` installs packages, as Isoloom's [`Installer`] says (CentOS 7's dnf family with
    /// `yum`, which it has instead). `None` for Windows, which a shell script installs nothing on.
    pub(super) fn of(os: Os) -> Option<Family> {
        match os.installer {
            Installer::Apt => Some(Family::Apt),
            Installer::Dnf if os.family == "centos" => Some(Family::Yum),
            Installer::Dnf => Some(Family::Dnf),
            Installer::Apk => Some(Family::Apk),
            Installer::Windows => None,
        }
    }
}

/// A service a port calls for: its package and unit on each family, and how it listens.
pub(super) struct Package {
    pub(super) port: u16,
    apt: &'static str,
    red_hat: &'static str,
    apk: &'static str,
    unit_apt: &'static str,
    unit_red_hat: &'static str,
    /// Lines run after installing (every family), to listen beyond localhost.
    pub(super) configure: &'static str,
}

impl Package {
    /// The package's name on `family`.
    pub(super) fn name(&self, family: Family) -> &'static str {
        match family {
            Family::Apt => self.apt,
            Family::Dnf | Family::Yum => self.red_hat,
            Family::Apk => self.apk,
        }
    }

    /// The service's unit on `family` (Alpine's OpenRC names it as Red Hat does).
    pub(super) fn unit(&self, family: Family) -> &'static str {
        match family {
            Family::Apt => self.unit_apt,
            Family::Dnf | Family::Yum | Family::Apk => self.unit_red_hat,
        }
    }
}

const POSTGRES_CONFIGURE: &str = r#"# Red Hat family: create the database cluster first (Debian's package does it itself).
if command -v postgresql-setup >/dev/null 2>&1 && [ ! -f /var/lib/pgsql/data/PG_VERSION ]; then
  postgresql-setup --initdb || runuser -u postgres -- initdb -D /var/lib/pgsql/data
fi
conf=$(find /etc/postgresql /var/lib/pgsql -name postgresql.conf 2>/dev/null | head -1 || true)
hba=$(find /etc/postgresql /var/lib/pgsql -name pg_hba.conf 2>/dev/null | head -1 || true)
if [ -n "$conf" ]; then sed -i "s/^#\?listen_addresses.*/listen_addresses = '*'/" "$conf"; fi
if [ -n "$hba" ] && ! grep -q "0.0.0.0/0" "$hba"; then echo "host all all 0.0.0.0/0 scram-sha-256" >> "$hba"; fi"#;

const MARIADB_CONFIGURE: &str = r#"for f in /etc/mysql/mariadb.conf.d/50-server.cnf /etc/my.cnf.d/mariadb-server.cnf; do
  if [ -f "$f" ]; then sed -i "s/^bind-address.*/bind-address = 0.0.0.0/" "$f"; fi
done"#;

const REDIS_CONFIGURE: &str = r#"for f in /etc/redis/redis.conf /etc/redis.conf; do
  if [ -f "$f" ]; then sed -i -e "s/^bind .*/bind 0.0.0.0/" -e "s/^protected-mode yes/protected-mode no/" "$f"; fi
done"#;

/// A package whose service needs nothing more than installing.
const fn plain(port: u16, apt: &'static str, red_hat: &'static str, apk: &'static str, units: (&'static str, &'static str)) -> Package {
    Package {
        port,
        apt,
        red_hat,
        apk,
        unit_apt: units.0,
        unit_red_hat: units.1,
        configure: "",
    }
}

/// The services the starters know, by port.
pub(super) const PACKAGES: &[Package] = &[
    plain(22, "openssh-server", "openssh-server", "openssh", ("ssh", "sshd")),
    plain(80, "nginx", "nginx", "nginx", ("nginx", "nginx")),
    plain(443, "nginx", "nginx", "nginx", ("nginx", "nginx")),
    Package {
        configure: POSTGRES_CONFIGURE,
        ..plain(5432, "postgresql", "postgresql-server", "postgresql16", ("postgresql", "postgresql"))
    },
    Package {
        configure: MARIADB_CONFIGURE,
        ..plain(3306, "mariadb-server", "mariadb-server", "mariadb", ("mariadb", "mariadb"))
    },
    Package {
        configure: REDIS_CONFIGURE,
        ..plain(6379, "redis-server", "redis", "redis", ("redis-server", "redis"))
    },
    plain(53, "bind9", "bind", "bind", ("named", "named")),
    plain(21, "vsftpd", "vsftpd", "vsftpd", ("vsftpd", "vsftpd")),
    plain(25, "postfix", "postfix", "postfix", ("postfix", "postfix")),
];

/// The packages answering on `ports`, each package once.
pub(super) fn wanted(ports: &[u16], family: Family) -> Vec<&'static Package> {
    let mut wanted: Vec<&Package> = Vec::new();
    for p in PACKAGES.iter().filter(|p| ports.contains(&p.port)) {
        if !wanted.iter().any(|w| w.name(family) == p.name(family)) {
            wanted.push(p);
        }
    }
    wanted
}

/// The lines that install `wanted` on `family`.
pub(super) fn install(wanted: &[&Package], family: Family) -> String {
    let names: Vec<&str> = wanted.iter().map(|p| p.name(family)).collect();
    let names = names.join(" ");
    match family {
        Family::Apt => format!("export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y {names}\n"),
        Family::Dnf => format!("dnf install -y {names}\n"),
        Family::Yum => format!("yum install -y {names}\n"),
        Family::Apk => format!("apk add --no-cache {names}\n"),
    }
}

/// The lines that enable and restart `wanted` where an init system runs (a VM, never a
/// container's image build, which has none).
pub(super) fn start(wanted: &[&Package], family: Family) -> String {
    let (guard, enable, restart) = match family {
        Family::Apk => (
            "command -v rc-service >/dev/null 2>&1 && [ -d /run/openrc ]",
            "rc-update add {} default",
            "rc-service {} restart",
        ),
        _ => ("[ -d /run/systemd/system ]", "systemctl enable {}", "systemctl restart {}"),
    };
    let units: Vec<String> = wanted
        .iter()
        .map(|p| p.unit(family))
        .map(|unit| format!("  {}\n  {}\n", enable.replace("{}", unit), restart.replace("{}", unit)))
        .collect();
    let units = units.concat();
    format!("# Start the services where an init system runs them (a VM; a container's build has none).\nif {guard}; then\n{units}fi\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(family: &'static str, installer: Installer) -> Os {
        Os {
            family,
            version: "1",
            name: family,
            installer,
        }
    }

    /// Regression: the family was guessed from the distribution's name, so one Isoloom adds got
    /// `apt-get` whatever it installs with.
    #[test]
    fn the_family_is_the_installer_isoloom_records() {
        assert_eq!(Family::of(os("newlinux", Installer::Dnf)), Some(Family::Dnf));
        assert_eq!(Family::of(os("newlinux", Installer::Apk)), Some(Family::Apk));
        assert_eq!(Family::of(os("centos", Installer::Dnf)), Some(Family::Yum));
        assert_eq!(Family::of(os("debian", Installer::Apt)), Some(Family::Apt));
        assert_eq!(Family::of(os("windows", Installer::Windows)), None);
    }
}
