#!/bin/sh
# Installs the isoloom binary from a GitHub release (macOS, Linux), after checking the archive
# against the release's SHA256SUMS.
#   curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/main/packaging/install.sh | sh
# ISOLOOM_VERSION picks a release (`v2.0.0` or `2.0.0`; default: the latest); ISOLOOM_INSTALL_DIR
# the folder (default: ~/.local/bin). The signature of SHA256SUMS and the build provenance are
# checked by hand: docs/guide/verify-a-release.md.
set -eu

repo="isoloom/isoloom"
version="${ISOLOOM_VERSION:-latest}"
dir="${ISOLOOM_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Linux) os="unknown-linux-musl" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "isoloom: no binary for $(uname -s); on Windows use the zip from https://github.com/$repo/releases, or cargo install --locked isoloom" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) echo "isoloom: no binary for $(uname -m); cargo install --locked isoloom builds one" >&2; exit 1 ;;
esac
name="isoloom-$arch-$os"
asset="$name.tar.gz"
case "$version" in
  latest) base="https://github.com/$repo/releases/latest/download" ;;
  v*) base="https://github.com/$repo/releases/download/$version" ;;
  *) base="https://github.com/$repo/releases/download/v$version" ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "Downloading $base/$asset"
curl -fsSL "$base/$asset" -o "$tmp/$asset"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
# The archive's line of SHA256SUMS, checked; a missing line is a failure, not a skip.
grep "  $asset\$" "$tmp/SHA256SUMS" > "$tmp/$asset.sha256" || {
  echo "isoloom: $asset isn't in the release's SHA256SUMS" >&2
  exit 1
}
(
  cd "$tmp"
  if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "$asset.sha256"; else shasum -a 256 -c "$asset.sha256"; fi
) >/dev/null
tar xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$dir"
install -m 0755 "$tmp/$name/isoloom" "$dir/isoloom"
echo "Installed $("$dir/isoloom" --version) to $dir/isoloom"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH." ;;
esac
