# Governance

Isoloom is an open-source project under the [Apache License 2.0](../LICENSE), led by its
maintainer.

## Maintainers

| Maintainer | GitHub | Areas |
|---|---|---|
| Florian Amette | [@florianamette](https://github.com/florianamette) | Everything: the spec, the targets, the catalogue, releases, security |

[CODEOWNERS](CODEOWNERS) says whose review a change needs.

## How decisions are made

- **Day to day.** Changes land through pull requests that pass CI ([CONTRIBUTING.md](CONTRIBUTING.md)).
  A maintainer reviews and merges them.
- **Larger decisions** (a new target, a spec change, a crate split, a policy change) are discussed
  in an issue or a [Discussion](https://github.com/isoloom/isoloom/discussions) first. The ones
  that shape the design are recorded as
  [architecture decision records](../docs/explanation/adr/README.md).
- **The maintainer decides** when a discussion doesn't reach agreement, and says why.

## Releases

A maintainer decides when to release and which version number it takes, following the
[versioning and stability policy](../docs/reference/policies.md). The steps are in
[Releasing](../docs/dev/releasing.md). Only maintainers push release tags.

## Security

Vulnerabilities are handled privately by the maintainers, as [SECURITY.md](SECURITY.md) says.

## Becoming a maintainer

A regular contributor whose changes have been merged over several months may be invited by the
maintainers. A maintainer who steps down, or is inactive for a year, moves to an emeritus list
here.

## Changing this document

Changes to this document go through a pull request that the maintainers approve.
