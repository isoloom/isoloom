#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# Written by Isoloom from the catalogue's postgres entry: the official image's entrypoint contract
# for the PostgreSQL project's packages. The first start initialises PGDATA with the superuser and
# its password, creates the database the POSTGRES_* settings name (from /etc/postgresql/isoloom.env,
# then the environment, a `_FILE` variable naming a file that holds the value), and runs
# /docker-entrypoint-initdb.d once (*.sh, *.sql, *.sql.gz); every start then runs the server in
# the foreground, as postgres.
set -eo pipefail

if [ -r /etc/postgresql/isoloom.env ]; then
  set -a
  . /etc/postgresql/isoloom.env
  set +a
fi

# A setting's value: POSTGRES_<name>, else the file POSTGRES_<name>_FILE names.
setting() {
  local plain="POSTGRES_$1" file="POSTGRES_$1_FILE"
  if [ -n "${!plain:-}" ]; then
    printf '%s' "${!plain}"
  elif [ -n "${!file:-}" ] && [ -r "${!file}" ]; then
    cat "${!file}"
  fi
}

case ${1:-postgres} in
  -*) set -- postgres "$@" ;;
  postgres) ;;
  *) exec "$@" ;;
esac

export PGDATA=${PGDATA:-/var/lib/postgresql/data}
mkdir -p "$PGDATA" /var/run/postgresql
chown -R postgres:postgres "$PGDATA" /var/run/postgresql
chmod 700 "$PGDATA"
as() { runuser -u postgres -- "$@"; }

if [ ! -s "$PGDATA/PG_VERSION" ]; then
  user=$(setting USER); user=${user:-postgres}
  password=$(setting PASSWORD)
  db=$(setting DB); db=${db:-$user}
  pwfile=$(mktemp)
  printf '%s' "$password" > "$pwfile"
  chown postgres "$pwfile"
  # shellcheck disable=SC2086 # POSTGRES_INITDB_ARGS is a list of arguments, as the image reads it.
  as initdb -D "$PGDATA" --username="$user" ${password:+--pwfile="$pwfile"} ${POSTGRES_INITDB_ARGS:-}
  rm -f "$pwfile"
  method=${POSTGRES_HOST_AUTH_METHOD:-$([ -n "$password" ] && echo scram-sha-256 || echo trust)}
  # PostgreSQL 13 and before hash passwords with md5 unless told otherwise.
  [ "$method" = scram-sha-256 ] && echo "password_encryption = 'scram-sha-256'" >> "$PGDATA/postgresql.conf"
  printf '\nhost all all all %s\n' "$method" >> "$PGDATA/pg_hba.conf"
  as pg_ctl -D "$PGDATA" -o "-c listen_addresses=''" -w start
  if [ -n "$password" ]; then
    # Read from its input: psql fills in :'pw' (quoted as a literal) there, never in a -c command.
    echo "ALTER USER \"$user\" PASSWORD :'pw'" | as psql -v ON_ERROR_STOP=1 --username "$user" --dbname postgres --set pw="$password" >/dev/null
  fi
  [ "$db" = postgres ] || as psql -v ON_ERROR_STOP=1 --username "$user" --dbname postgres -c "CREATE DATABASE \"$db\""
  for f in /docker-entrypoint-initdb.d/*; do
    case $f in
      *.sh) if [ -x "$f" ]; then "$f"; else . "$f"; fi ;;
      *.sql) as psql -v ON_ERROR_STOP=1 --username "$user" --dbname "$db" -f "$f" ;;
      *.sql.gz) gunzip -c "$f" | as psql -v ON_ERROR_STOP=1 --username "$user" --dbname "$db" ;;
    esac
  done
  as pg_ctl -D "$PGDATA" -m fast -w stop
fi

shift || true
exec runuser -u postgres -- postgres -c listen_addresses='*' "$@"
