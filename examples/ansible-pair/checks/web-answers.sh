#!/bin/sh
# The page the playbook wrote, served by web.
set -eu
curl -fsS --max-time 10 http://web/ | grep -q "hello from ansible"
echo "web answers with the playbook's page"
