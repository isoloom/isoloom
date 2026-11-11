# Releasing

How a maintainer releases Isoloom: the crates to crates.io and the `isoloom` binaries to a GitHub
release, both from one tag `vX.Y.Z`. Only maintainers push release tags
([governance](../../.github/GOVERNANCE.md)).

## What a release publishes

| What | Workflow | Where |
|---|---|---|
| The 13 published crates | [`publish-crates.yml`](../../.github/workflows/publish-crates.yml) | crates.io, through Trusted Publishing (OIDC, no stored token) |
| The `isoloom` binaries for five targets, the SBOM, `SHA256SUMS`, its Sigstore signature, build provenance | [`release.yml`](../../.github/workflows/release.yml) | the GitHub release of the tag |

Both start with the release gate ([`gate.yml`](../../.github/workflows/gate.yml): rustfmt, clippy,
the tests, the API docs, the quality ratchet). A tag that fails it publishes nothing.

`release.yml` then:

1. checks that the tag is `v` + the workspace version, and takes the release notes from the
   CHANGELOG's `## [X.Y.Z]` section (it fails when there is none);
2. builds `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`,
   `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`, each archive holding the binary,
   `LICENSE`, `NOTICE`, `README.md` and `THIRD-PARTY-LICENSES.md` (cargo-about);
3. writes the CycloneDX SBOM (`isoloom.cdx.json`) and `SHA256SUMS`, signs `SHA256SUMS` keylessly
   with cosign and attests the provenance of every file;
4. creates the GitHub release, in the `release` environment (where approvals can be required);
5. installs the published release with `packaging/install.sh` (Linux x86_64 and arm64, macOS) and
   with the Action (Linux, macOS, Windows), runs it, and checks the signature and the provenance
   as [verify a release](../guide/verify-a-release.md) says. A failure here means the published
   release is broken: fix it with a patch release;
6. waits (up to 40 minutes) for the website to serve `schemas/spec-v1.schema.json` at its `$id`
   (`https://www.isoloom.com/schema/v1.json`) and every `www.isoloom.com` page the CLI and the
   schema link to (`/en/docs/configuration`, `/en/docs/spec`), and fails when one doesn't answer
   or the served schema differs. A failure here means editors and `--help` point to stale or dead
   pages: publish the website (below).

The asset names (`isoloom-<target>.tar.gz`, `.zip` on Windows) are read by `install.sh`, the
Action, cargo-binstall and the verification guide: change them together.

## Before the tag

1. **Version.** Raise `[workspace.package].version` and every `version` of the
   `[workspace.dependencies]` path dependencies (crates.io refuses a path dependency without
   one). The [policies](../reference/policies.md) say which part to raise; CI's `semver` job
   fails when a supported crate breaks its API without a major.
2. **CHANGELOG.** Rename `## [Unreleased]` to `## [X.Y.Z] — YYYY-MM-DD`, with breaking items
   marked **Breaking:** and, for a major, a migration guide under `docs/guide/`. Open a new empty
   `## [Unreleased]`.
3. **Generated pages.** `cargo run -q -p isoloom -- docs --spec > docs/spec.md`, the same with
   `--matrix > docs/matrix.md`, and `docs/reference/cli.md`; `git diff --exit-code docs`.
4. **Docs that name the version.** The examples in `README.md`, `action.yml`,
   `packaging/install.sh` and the guides name `vX.Y.Z` where a version is shown: update them for a
   new major.
5. **Green CI on `main`** for the commit you tag, including the Compose and Kubernetes runs.
6. **Dry run** the crates (below) when dependencies or packaging changed.

## Tag

```sh
git tag -a vX.Y.Z -m "Isoloom X.Y.Z"    # -s instead of -a to sign it
git push origin vX.Y.Z
```

Then watch both workflows. For a major release, move the Action's major tag once the release is
verified, so `isoloom/isoloom@vX` users get it:

```sh
git tag -f vX vX.Y.Z && git push -f origin vX
```

Publish Isoloom **before** the crates built on it (a runner, an app): they depend on Isoloom's
crates from crates.io at this version.

## Publish the website

The website (its own repository) builds its docs from this repository's `docs/` and serves the
committed schemas at their `$id`. The spec's `$id`, every example's modeline and the CLI's help
link there, so a release isn't done until the website shows it:

1. Once the tag is pushed, sync the website to the tag's `docs/` and `schemas/`, and deploy it.
2. Check that the schema URL and the CLI's doc links answer:

   ```sh
   curl -fsSL https://www.isoloom.com/schema/v1.json | jq -S . | diff - <(jq -S . schemas/spec-v1.schema.json)
   curl -fsS -o /dev/null https://www.isoloom.com/en/docs/configuration
   curl -fsS -o /dev/null https://www.isoloom.com/en/docs/spec
   ```

`release.yml`'s `website` job runs the same check on every `www.isoloom.com` link in the CLI's
and the schema's source, waiting for the deploy.

## The crates

All crates share one version. They are published in dependency order, and the workflow skips any
crate whose version is already on crates.io, so re-running it after a partial failure is safe:

1. `isoloom-manifest`, 2. `isoloom-spec`, 3. `isoloom-catalogue`, 4. `isoloom-plan`,
5. `isoloom-render`, 6. `isoloom-compose`, 7. `isoloom-kubernetes`, 8. `isoloom-terraform`,
9. `isoloom-vagrant`, 10. `isoloom-ansible`, 11. `isoloom-targets`, 12. `isoloom-import`,
13. `isoloom` (the command line).

Not published (`publish = false`, the workspace default): `xtask`, `xtask-quality` and the fuzz
workspace.

### Trusted Publishing

No long-lived token is stored. `publish-crates.yml` runs in the `crates-io` environment with
`id-token: write`; `rust-lang/crates-io-auth-action` exchanges the OIDC token for a short-lived
crates.io token. Each crate's *Settings → Trusted Publishing* on crates.io names repository
`isoloom/isoloom`, workflow `publish-crates.yml` and environment `crates-io`.

A **new crate name** can't be created by Trusted Publishing: publish its first version by hand
(`cargo login`, `cargo publish -p <crate>`), then add the Trusted Publisher.

### The catalogue in a package

`isoloom-catalogue`'s `build.rs` embeds the repository's `catalogue/` folder, which lives outside
the crate. The crate carries a symlink `catalogue -> ../../../catalogue`: cargo packages the files
it points at, and `build.rs` reads that copy when the repository's isn't there.

### Dry run

```sh
cargo publish --dry-run -p isoloom-manifest
```

The other crates (`isoloom-spec` included: it builds on `isoloom-manifest`) depend on sibling crates that are not on crates.io at the new version until they
are published: point those at
the checkout with a `[patch.crates-io]` table passed to cargo
(`cargo --config patch.toml publish --dry-run -p <crate>`).

To publish the crates without a tag, run **Publish crates** from the Actions tab.

## After the release

- Check the release page: five archives, `isoloom.cdx.json`, `SHA256SUMS`,
  `SHA256SUMS.sigstore.json`, and the notes.
- Check that `release.yml`'s `website` job passed ([publish the website](#publish-the-website)).
- Bump the consumers (Isothrum's `isoloom-*` dependencies) when the run manifest or the supported
  API changed.
- A security fix: publish the GitHub security advisory ([SECURITY.md](../../.github/SECURITY.md)).
