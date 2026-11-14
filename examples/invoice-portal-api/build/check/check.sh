#!/bin/sh
# The intended path, from the player's side: the documented key gets invoice INV-1042 with its
# amount. Never prints the amount: it is the answer.
set -eu
API=http://web/api/invoices/INV-1042
body=$(curl -fsS --max-time 10 -H 'X-Portal-Key: example-demo-key' "$API") || {
  echo "the lookup with the documented key failed"
  exit 1
}
echo "$body" | grep -q '"amount"' || { echo "INV-1042 came back without an amount"; exit 1; }
echo "the documented lookup works"
