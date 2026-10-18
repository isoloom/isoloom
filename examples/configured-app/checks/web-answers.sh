#!/bin/sh
# The site answers, and its proxy reaches the API.
set -eu
wget -qO- "http://web:80/" | grep -q "configured by isoloom"
wget -qO- "http://web:80/api/" | grep -q "api ok"
