#!/bin/sh
# IIS serves the page and the filled-in template.
set -eu
wget -qO- "http://iis:80/" | grep -q "configured by isoloom"
wget -qO- "http://iis:80/about.txt" | grep -q "^iis at .*, configured by isoloom"
