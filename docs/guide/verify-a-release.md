# Verify a release

Each [GitHub release](https://github.com/isoloom/isoloom/releases) from 2.0.0 on carries, next to
the archives:

| File | What it is |
|---|---|
| `isoloom-<target>.tar.gz` / `.zip` | A folder `isoloom-<target>/` with the binary, `LICENSE`, `NOTICE`, `README.md` and `THIRD-PARTY-LICENSES.md` |
| `isoloom.cdx.json` | The CycloneDX SBOM of the `isoloom` binary's dependencies |
| `SHA256SUMS` | The SHA-256 of every file above |
| `SHA256SUMS.sigstore.json` | A keyless Sigstore signature of `SHA256SUMS` (a cosign bundle) |

Every file listed in `SHA256SUMS` also has a build provenance attestation on GitHub. The release
workflow is [`.github/workflows/release.yml`](../../.github/workflows/release.yml); it runs only
on a version tag, after the test gate, and installs the published release before it finishes.

Targets: `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` (static Linux binaries),
`x86_64-apple-darwin`, `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`.

The 1.x releases have no binaries: install a 1.x with `cargo install --locked isoloom@1`.

## Download

```sh
VERSION=2.0.0                      # the release you want
TARGET=x86_64-unknown-linux-musl   # your platform
BASE="https://github.com/isoloom/isoloom/releases/download/v${VERSION}"
curl -fsSLO "${BASE}/isoloom-${TARGET}.tar.gz"
curl -fsSLO "${BASE}/SHA256SUMS"
curl -fsSLO "${BASE}/SHA256SUMS.sigstore.json"
```

## 1. Check the signature of SHA256SUMS

With a recent [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) (the
release workflow signs with cosign 3):

```sh
cosign verify-blob SHA256SUMS \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-identity "https://github.com/isoloom/isoloom/.github/workflows/release.yml@refs/tags/v${VERSION}" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

`Verified OK` means `SHA256SUMS` was signed by the release workflow of `isoloom/isoloom`, running
for that tag, and the signature is in Sigstore's public transparency log.

## 2. Check the archive against SHA256SUMS

```sh
sha256sum --check --ignore-missing SHA256SUMS            # Linux
shasum -a 256 --check --ignore-missing SHA256SUMS        # macOS
```

On Windows (PowerShell), compare the hash by hand:

```powershell
(Get-FileHash .\isoloom-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash.ToLower()
Select-String "isoloom-x86_64-pc-windows-msvc.zip" .\SHA256SUMS
```

## 3. Check the build provenance

With the [GitHub CLI](https://cli.github.com/):

```sh
gh attestation verify "isoloom-${TARGET}.tar.gz" \
  --repo isoloom/isoloom \
  --signer-workflow isoloom/isoloom/.github/workflows/release.yml
```

This shows the file was built by that workflow from the tagged commit of the repository.

## 4. Install

```sh
tar -xzf "isoloom-${TARGET}.tar.gz"
install "isoloom-${TARGET}/isoloom" ~/.local/bin/
isoloom --version
```

What the other ways of installing check:

| Way | Checks |
|---|---|
| `packaging/install.sh` | The archive against `SHA256SUMS` (not the signature) |
| The GitHub Action (`isoloom/isoloom@<ref>`) | The archive against `SHA256SUMS`, on Linux, macOS and Windows |
| `cargo binstall isoloom` | Neither the signature nor the provenance |
| `cargo install --locked isoloom` | Builds from the crates on crates.io, with the versions `Cargo.lock` pins |

Verify as above when your policy needs the signature or the provenance.
