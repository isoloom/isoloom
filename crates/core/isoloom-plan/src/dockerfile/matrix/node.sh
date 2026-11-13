# Node.js {VERSION} as node, npm and npx, as the node:{VERSION} image has it: nodejs.org's
# release for this CPU, its checksum checked against the release's list.
if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends ca-certificates curl xz-utils
fi
case $(uname -m) in x86_64) arch=x64 ;; aarch64) arch=arm64 ;; *) echo "no Node.js build for $(uname -m)" >&2; exit 1 ;; esac
case {VERSION} in
  *.*.*) dir=v{VERSION} ;;
  *) dir=latest-v$(echo {VERSION} | cut -d. -f1).x ;;
esac
curl -fsSL "https://nodejs.org/dist/$dir/SHASUMS256.txt" -o /tmp/node.sums
file=$(grep -E "node-v[0-9.]+-linux-$arch\.tar\.(xz|gz)$" /tmp/node.sums | grep '\.xz$' | head -n1 | awk '{print $2}')
[ -n "$file" ] || file=$(grep -E "node-v[0-9.]+-linux-$arch\.tar\.gz$" /tmp/node.sums | head -n1 | awk '{print $2}')
curl -fsSL "https://nodejs.org/dist/$dir/$file" -o "/tmp/$file"
(cd /tmp && grep " $file\$" node.sums | sha256sum -c -)
tar -xf "/tmp/$file" --strip-components=1 -C /usr/local --no-same-owner
rm -f "/tmp/$file" /tmp/node.sums
getent group node >/dev/null || groupadd -g 1000 node 2>/dev/null || groupadd node
getent passwd node >/dev/null || useradd -u 1000 -g node -s /bin/bash -m node 2>/dev/null || useradd -g node -s /bin/bash -m node
