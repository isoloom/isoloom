#!/bin/sh
# The writer's data, in the shared volume.
set -eu
echo "written by the writer" > /srv/shared/greeting
