# MariaDB {VERSION}, as the mariadb:{VERSION} image has it: Debian 12's own 10.11, else MariaDB's
# repository for that release (dnf on the RHEL family).
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl
  if [ "{VERSION}" != 10.11 ]; then
    curl -fsSL https://r.mariadb.com/downloads/mariadb_repo_setup -o /tmp/mariadb_repo_setup
    bash /tmp/mariadb_repo_setup --mariadb-server-version="mariadb-{VERSION}" --skip-maxscale --skip-tools
  fi
  apt-get install -y --no-install-recommends mariadb-server mariadb-client
  conf=/etc/mysql/mariadb.conf.d/99-isoloom.cnf
else
  curl -fsSL https://r.mariadb.com/downloads/mariadb_repo_setup -o /tmp/mariadb_repo_setup
  bash /tmp/mariadb_repo_setup --mariadb-server-version="mariadb-{VERSION}" --skip-maxscale --skip-tools
  dnf install -y MariaDB-server MariaDB-client
  conf=/etc/my.cnf.d/99-isoloom.cnf
fi
# The image listens on every address and keeps no host cache.
printf '[mysqld]\nbind-address = 0.0.0.0\nskip-name-resolve\n' > "$conf"
ln -sf "$(command -v mariadbd || command -v mysqld)" /usr/local/bin/mysqld 2>/dev/null || true
ln -sf /usr/local/bin/mysqld /usr/local/bin/mariadbd 2>/dev/null || true
mkdir -p /docker-entrypoint-initdb.d /run/mysqld
chown mysql:mysql /run/mysqld
