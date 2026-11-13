# PostgreSQL {VERSION}, as the postgres:{VERSION} image has it: the PostgreSQL project's
# repository for that release (apt.postgresql.org; dnf on the RHEL family), with the image's
# entrypoint.
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl gnupg lsb-release
  curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor -o /usr/share/keyrings/pgdg.gpg
  echo "deb [signed-by=/usr/share/keyrings/pgdg.gpg] https://apt.postgresql.org/pub/repos/apt $(lsb_release -sc)-pgdg main {VERSION}" > /etc/apt/sources.list.d/pgdg.list
  apt-get update
  apt-get install -y --no-install-recommends postgresql-{VERSION} postgresql-client-{VERSION}
  bin=/usr/lib/postgresql/{VERSION}/bin
else
  dnf install -y "https://download.postgresql.org/pub/repos/yum/reporpms/EL-$(rpm -E %rhel)-x86_64/pgdg-redhat-repo-latest.noarch.rpm"
  dnf -qy module disable postgresql || true
  dnf install -y postgresql{VERSION}-server
  bin=/usr/pgsql-{VERSION}/bin
fi
for b in "$bin"/*; do ln -sf "$b" /usr/local/bin/; done
mkdir -p /docker-entrypoint-initdb.d /var/lib/postgresql/data /var/run/postgresql
chown -R postgres:postgres /var/lib/postgresql /var/run/postgresql
cat > /usr/local/bin/docker-entrypoint.sh <<'SHIM'
#!/bin/bash
set -eo pipefail
[ "${1:-postgres}" = postgres ] || exec "$@"
export PGDATA=${PGDATA:-/var/lib/postgresql/data}
mkdir -p "$PGDATA" /var/run/postgresql
chown -R postgres:postgres "$PGDATA" /var/run/postgresql
chmod 700 "$PGDATA"
as() { runuser -u postgres -- "$@"; }
if [ ! -s "$PGDATA/PG_VERSION" ]; then
  user=${POSTGRES_USER:-postgres}
  pw=$(mktemp); printf '%s' "${POSTGRES_PASSWORD:-}" > "$pw"; chown postgres "$pw"
  as initdb -D "$PGDATA" --username="$user" ${POSTGRES_PASSWORD:+--pwfile="$pw"} ${POSTGRES_INITDB_ARGS:-}
  rm -f "$pw"
  printf "\nhost all all all %s\n" "${POSTGRES_HOST_AUTH_METHOD:-$([ -n "${POSTGRES_PASSWORD:-}" ] && echo scram-sha-256 || echo trust)}" >> "$PGDATA/pg_hba.conf"
  as pg_ctl -D "$PGDATA" -o "-c listen_addresses=''" -w start
  db=${POSTGRES_DB:-$user}
  [ "$db" = postgres ] || as psql -v ON_ERROR_STOP=1 --username "$user" --dbname postgres -c "CREATE DATABASE \"$db\""
  for f in /docker-entrypoint-initdb.d/*; do
    case $f in
      *.sh) . "$f" ;;
      *.sql) as psql -v ON_ERROR_STOP=1 --username "$user" --dbname "$db" -f "$f" ;;
      *.sql.gz) gunzip -c "$f" | as psql -v ON_ERROR_STOP=1 --username "$user" --dbname "$db" ;;
    esac
  done
  as pg_ctl -D "$PGDATA" -m fast -w stop
fi
exec runuser -u postgres -- "$@" -c listen_addresses='*'
SHIM
chmod 755 /usr/local/bin/docker-entrypoint.sh
