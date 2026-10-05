#!/bin/sh
# Ensure the SSH service is present and running (the box ships it; this makes it explicit).
set -e
systemctl enable --now ssh
