#!/bin/sh
# Both of the API's services answer.
set -eu
wget -qO- "http://api:8080/" | grep -q "api ok"
wget -qO- "http://api:8081/" | grep -q "api ok"
