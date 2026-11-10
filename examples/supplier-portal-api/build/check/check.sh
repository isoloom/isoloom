#!/bin/sh
# The portal answers on its port with its page.
set -eu
curl -fsS --max-time 10 http://web:8206/ | grep -q "Supplier portal"
echo "the portal answers"
