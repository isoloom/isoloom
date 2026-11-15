## What changed

<!-- What a user or embedder notices, and why. -->

Closes #

## Verified

<!-- Paste the gate results and, for a change to Compose, the plan or the runtime, the real run. -->

- [ ] `cargo xtask check` is green (its summary pasted above; `--fast` while iterating)
- [ ] docs: a user-visible change is in the CHANGELOG and the guide it affects
- [ ] `docs/spec.md` regenerated, and `git diff --exit-code docs` is clean unless the change is the point
- [ ] snapshot changes reviewed (`cargo insta review`) and each kind of difference explained below
- [ ] no secrets: no token, password, key or private address in the diff, the snapshots, the examples or the pasted output

## Left out

<!-- What this deliberately doesn't do, and where it is tracked. -->
