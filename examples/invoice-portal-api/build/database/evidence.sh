#!/bin/sh
# The database's setup step, the same on containers and VMs: places the amount of invoice
# INV-1042 once the database answers. The amount is kept in the `evidence` volume, so a restart
# places the same one. Without a launch token (a local run, CI) it is the development amount;
# with one, it is derived from the token here (a real lab would claim it from CTF_API_URL). The
# token is read from its file, never from the environment, and never printed.
set -eu
KEPT=/var/lib/ctf/evidence
TOKEN=/run/lab/launch-token
mkdir -p "$(dirname "$KEPT")"

if [ ! -s "$KEPT" ]; then
  if [ -s "$TOKEN" ]; then
    sum=$(cksum < "$TOKEN" | cut -d' ' -f1)
    echo "$((1000 + sum % 99000)).00" > "$KEPT"
    echo "evidence: derived from the launch token (API: ${CTF_API_URL:-none given})"
  else
    echo "1234.00" > "$KEPT"
    echo "evidence: the development amount"
  fi
fi
amount=$(cat "$KEPT")
case "$amount" in
  '' | *[!0-9.]*) echo "evidence: unexpected amount" >&2; exit 1 ;;
esac

i=0
until mysql -h 127.0.0.1 -P 3207 -uroot -pexample-root-password -e 'SELECT 1' >/dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -lt 90 ] || { echo "evidence: the database never answered" >&2; exit 1; }
  sleep 2
done
mysql -h 127.0.0.1 -P 3207 -uroot -pexample-root-password portal \
  -e "UPDATE invoices SET amount = $amount WHERE reference = 'INV-1042';"
echo "evidence: placed"
