# kCTF's challenge base on a VM (the owner's decision: the VM is the jail, no nsjail): socat, and
# the base's commands as shims. `nsjail ... -- <command>` runs the command as the challenge's
# `user` in its home; `kctf_pow` runs its command (no proof of work); `kctf_setup` has nothing to
# set up.
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends socat util-linux
cat > /usr/local/bin/nsjail <<'SHIM'
#!/bin/sh
# kCTF's jail, on a VM that is the jail: the command after `--`, as the challenge's user.
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do shift; done
[ "$#" -gt 0 ] && shift
cd /home/user 2>/dev/null || cd /
if id user >/dev/null 2>&1; then
  exec setpriv --reuid "$(id -u user)" --regid "$(id -g user)" --init-groups -- "$@"
fi
exec setpriv --reuid 1000 --regid 1000 --clear-groups -- "$@"
SHIM
printf '#!/bin/sh\nexec "$@"\n' > /usr/local/bin/kctf_pow
printf '#!/bin/sh\nexit 0\n' > /usr/local/bin/kctf_setup
chmod 755 /usr/local/bin/nsjail /usr/local/bin/kctf_pow /usr/local/bin/kctf_setup
