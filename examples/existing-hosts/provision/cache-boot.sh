#!/bin/sh
# Redis wants transparent hugepages off (latency spikes, memory use). That's a kernel boot
# option: set here, in effect after the `reboot` step that follows this one.
set -eu
mkdir -p /etc/default/grub.d
echo 'GRUB_CMDLINE_LINUX_DEFAULT="$GRUB_CMDLINE_LINUX_DEFAULT transparent_hugepage=never"' > /etc/default/grub.d/90-redis.cfg
update-grub
