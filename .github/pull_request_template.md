## What changed

<!-- What a user or embedder notices, and why. -->

Closes #

## Verified

<!-- Paste the gate results and, for a change to Compose, the plan or the runtime, the real run. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps`
- [ ] `cargo machete`
- [ ] `cargo deny check`
- [ ] `typos`
- [ ] `docs/spec.md` regenerated, and `git diff --exit-code docs` is clean unless the change is the point
- [ ] snapshot changes reviewed (`cargo insta review`) and each kind of difference explained below
- [ ] a CHANGELOG line, if a user or embedder notices the change ("Breaking (library)" for the supported surface)
- [ ] no secrets: no token, password, key or private address in the diff, the snapshots, the examples or the pasted output

## Left out

<!-- What this deliberately doesn't do, and where it is tracked. -->
