# Use Isoloom in CI

Two jobs are common: checking that the committed files under `.isoloom/` match the spec, and
generating the files to run an environment for the length of a job.

## Install it with the GitHub Action

The repository is a GitHub Action that installs a release binary, checks it against the release's
`SHA256SUMS` and puts `isoloom` on the `PATH`, on Linux, macOS and Windows runners:

```yaml
steps:
  - uses: actions/checkout@<commit sha>  # v7
  - uses: isoloom/isoloom@<commit sha>   # v2.1.1
    with:
      version: v2.1.1
  - run: isoloom check
```

| Input | Default | Meaning |
|---|---|---|
| `version` | `latest` | The release to install (`v2.1.1`), or `latest` |

### Pinning

- **To a commit SHA** (recommended, and what most security policies require): the Action's code
  can't change under you. Pin `version` too, so the binary it installs doesn't change either.
  Dependabot's `github-actions` updates keep both current when the comment names the version.
- **To a major tag** (`isoloom/isoloom@v2`): you get every 2.x fix of the Action without
  editing the workflow, at the price of trusting the tag. With `version: latest` you also get
  every new release of the binary.
- Never pin to `main`.

Without the Action: `curl -fsSL https://raw.githubusercontent.com/isoloom/isoloom/<ref>/packaging/install.sh | ISOLOOM_VERSION=v2.1.1 sh`,
or `cargo install --locked isoloom`.

## Check the generated files

When you commit `.isoloom/` ([why and how](commit-generated-files.md)), `isoloom check` fails
when the files no longer match the spec:

```yaml
  - run: isoloom check            # exit 1 when a file is out of date or stale
```

`isoloom check --target docker` checks one target's files, `--profile` a profile's.
`isoloom check --quiet` prints only the problems, and `isoloom check --json` prints, for a tool,
each file's status (`current`, `out_of_date`, `missing`, `unreadable`, `no_longer_generated`), the
reasons a target asked for has no files, and a summary. Exit codes: 0 up to date, 1 out of date (or the spec is invalid), 2 could not run.

Validate every spec of a repository without writing anything:

```yaml
  - run: |
      for spec in $(git ls-files '*isoloom.yml'); do isoloom validate "$(dirname "$spec")"; done
```

## Run an environment in a job

Generate the files in the job, then hand them to a runner:

```yaml
  - run: isoloom generate --target docker
  - run: isothrum up . --target docker
  - run: your-tests-against-the-environment
  - if: always()
    run: isothrum down . --target docker
```

Isothrum's own [CI guide](https://github.com/Isothrum/isothrum/blob/main/docs/how-to/ci.md)
covers installing it, cloud targets and carrying state between jobs. Generation needs no network
and no credentials, so it can run in a job without either ([offline](offline.md)).
