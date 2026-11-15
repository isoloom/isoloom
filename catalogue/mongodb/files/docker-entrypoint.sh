#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# Written by Isoloom from the catalogue's mongodb entry: the official image's entrypoint contract
# for MongoDB's own build. The image's environment comes from /etc/isoloom/image.env when a
# translated Dockerfile wrote one, then from the service's own. The first start, on an empty
# /data/db, creates the root user MONGO_INITDB_ROOT_USERNAME names (its password from
# MONGO_INITDB_ROOT_PASSWORD, or the file MONGO_INITDB_ROOT_PASSWORD_FILE names) and runs
# /docker-entrypoint-initdb.d (*.js through the shell the build ships, *.sh); every start then
# runs mongod in the foreground on every address, with authentication when there is a root user.
set -eo pipefail

if [ -r /etc/isoloom/image.env ]; then
  set -a
  . /etc/isoloom/image.env
  set +a
fi

case ${1:-mongod} in
  -*) set -- mongod "$@" ;;
  mongod) ;;
  *) exec "$@" ;;
esac
shift || true

datadir=/data/db
mkdir -p "$datadir"
chown -R mongodb:mongodb "$datadir"
shell=$(command -v mongosh || command -v mongo || true)
as_mongodb() { setpriv --reuid mongodb --regid mongodb --init-groups -- "$@"; }
user=${MONGO_INITDB_ROOT_USERNAME:-}
password=${MONGO_INITDB_ROOT_PASSWORD:-}
if [ -z "$password" ] && [ -n "${MONGO_INITDB_ROOT_PASSWORD_FILE:-}" ] && [ -r "$MONGO_INITDB_ROOT_PASSWORD_FILE" ]; then
  password=$(cat "$MONGO_INITDB_ROOT_PASSWORD_FILE")
fi

if [ -z "$(ls -A "$datadir")" ] && { [ -n "$user" ] || [ -n "$(ls -A /docker-entrypoint-initdb.d 2>/dev/null)" ]; }; then
  as_mongodb mongod --dbpath "$datadir" --bind_ip 127.0.0.1 --port 27017 --fork --logpath /tmp/mongod-init.log
  db=${MONGO_INITDB_DATABASE:-test}
  if [ -n "$user" ]; then
    [ -n "$shell" ] || { echo "mongodb: no shell in this build to create the root user" >&2; exit 1; }
    # The values as JavaScript strings: backslashes and single quotes escaped.
    js() { printf "'%s'" "$(printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e "s/'/\\\\'/g")"; }
    "$shell" --quiet admin --eval "db.createUser({user: $(js "$user"), pwd: $(js "$password"), roles: [{role: 'root', db: 'admin'}]})"
  fi
  for f in /docker-entrypoint-initdb.d/*; do
    case $f in
      *.sh) if [ -x "$f" ]; then "$f"; else . "$f"; fi ;;
      *.js)
        [ -n "$shell" ] || { echo "mongodb: no shell in this build to run $f" >&2; exit 1; }
        "$shell" --quiet "$db" "$f" ;;
    esac
  done
  as_mongodb mongod --dbpath "$datadir" --shutdown
fi

auth=()
[ -z "$user" ] || auth=(--auth)
exec setpriv --reuid mongodb --regid mongodb --init-groups -- mongod --dbpath "$datadir" --bind_ip_all "${auth[@]}" "$@"
