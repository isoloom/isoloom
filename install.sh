#!/bin/sh
# Installs the isoloom binary from the GitHub releases (macOS, Linux).
#   curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/install.sh | sh
# ISOLOOM_VERSION=v0.6.0 picks a release (default: the latest); ISOLOOM_INSTALL_DIR the folder
# (default: ~/.local/bin).
set -eu

repo="isoloom/isoloom"
version="${ISOLOOM_VERSION:-latest}"
dir="${ISOLOOM_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Linux) os="unknown-linux-musl" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "isoloom: no binary for $(uname -s); build it with cargo install --git https://github.com/$repo isoloom" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) echo "isoloom: no binary for $(uname -m)" >&2; exit 1 ;;
esac
asset="isoloom-$arch-$os.tar.gz"
if [ "$version" = "latest" ]; then
  url="https://github.com/$repo/releases/latest/download/$asset"
else
  url="https://github.com/$repo/releases/download/$version/$asset"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $url"
curl -fsSL "$url" -o "$tmp/$asset"
curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"
(cd "$tmp" && if command -v sha256sum >/dev/null; then sha256sum -c "$asset.sha256"; else shasum -a 256 -c "$asset.sha256"; fi) >/dev/null
tar xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$dir"
install -m 0755 "$tmp/isoloom" "$dir/isoloom"
echo "Installed $("$dir/isoloom" --version) to $dir/isoloom"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH." ;;
esac
