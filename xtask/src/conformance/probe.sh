#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# The state the conformance recipe reaches, one step kind per line: `ok <kind>`, or
# `FAIL <kind>: <why>`. Read by `cargo xtask conformance`; its repository line names the family's
# package manager (apt, rpm or apk) as its first argument.
kind=$1
check() {
  name=$1
  shift
  if out=$("$@" 2>&1); then echo "ok $name"; else echo "FAIL $name: $(printf '%s' "$out" | tail -n 1)"; fi
}
repository() {
  case $kind in
    apt) test -s /usr/share/keyrings/isoloom-nginx.asc && grep -q '^URIs: https://nginx.org/' /etc/apt/sources.list.d/isoloom-nginx.sources && apt-get update -qq ;;
    rpm) test -s /etc/pki/rpm-gpg/isoloom-nginx.asc && dnf -q makecache --repo isoloom-nginx ;;
    apk) test -s /etc/apk/keys/nginx_signing.rsa.pub && grep -q '^https://nginx.org/' /etc/apk/repositories && apk update -q ;;
  esac
}
packages() {
  command -v curl >/dev/null
}
group() {
  getent group isoloom-ops | grep -q ':1600:'
}
user() {
  getent passwd isoloom-alice | grep -q ':1600:1600:.*:/bin/sh$' &&
    id -nG isoloom-alice | tr ' ' '\n' | grep -qx isoloom-ops &&
    awk -F : '$1 == "isoloom-alice" { exit !($2 ~ /^\$6\$/) }' /etc/shadow
}
file() {
  d=/srv/conformance
  test -d $d/dir && [ "$(stat -c '%U %a' $d/dir)" = "isoloom-alice 750" ] &&
    [ "$(readlink $d/link)" = $d/dir ] &&
    [ "$(sha256sum $d/download | cut -d ' ' -f 1)" = cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30 ] &&
    test ! -e $d/gone
}
copy() {
  d=/srv/conformance
  [ "$(cat $d/one.conf)" = one ] && [ "$(stat -c '%U %a' $d/one.conf)" = "isoloom-alice 640" ] &&
    [ "$(cat $d/tree/a.txt)" = a ] && [ "$(cat $d/tree/sub/b.txt)" = b ]
}
template() {
  d=/srv/conformance
  [ "$(cat $d/app.conf)" = "name = m" ] && [ "$(stat -c '%G %a' $d/app.conf)" = "isoloom-ops 640" ]
}
command_() {
  test -e /srv/conformance/made && [ "$(cat /srv/conformance/unless.log)" = ran ]
}
check repository repository
check packages packages
check group group
check user user
check file file
check copy copy
check template template
check command command_
