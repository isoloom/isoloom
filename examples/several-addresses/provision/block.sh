#!/bin/sh
# The block host: nothing to install, its addresses are the point (the set-up adds them).
set -eu
ip -o -4 addr show
