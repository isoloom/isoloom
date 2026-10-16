# Docs alignment report (Phase 7 gate)

Every user-visible fact in `plan/docs-facts.md` reconciled against the shipped docs: where it is
documented, or why it is struck as obsolete. This is the Phase 7 docs-alignment gate.

**Result: PASS.** Every fact is reflected in a doc, enforced by a generated-doc stale test, or
carried as an honest "owed / known limitation". The owed items (below) are documented *as owed*,
which the gate requires ("no claim without proof"). No fact overclaims.

## Where the docs live

| Surface | What it covers | How it stays true |
|---|---|---|
| `docs/spec.md` | every spec field, type, default, allowed values, limits | generated (`isoloom docs --spec`); stale test `docs_spec_is_up_to_date` |
| `docs/matrix.md` | each target × each field decision + the tier table with evidence | generated (`isoloom docs --matrix`); stale test `docs_matrix_md_is_what_isoloom_docs_matrix_prints` |
| `schemas/spec-v1.schema.json` | the spec schema incl. catalogue `with:` | written by the catalogue; stale test in `isoloom-spec/tests/schema.rs` |
| `schemas/run-manifest-v0.schema.json` | the run manifest | `isoloom-manifest`; stale test in `isoloom-manifest/tests/schema.rs` |
| `catalogue/{entry,contract}.schema.json` | catalogue entry/contract types | generated; `isoloom-catalogue` tests |
| `README.md` | what Isoloom is, install, targets, crates.io | audited below; docs test checks its spec block + links |
| `docs/getting-started.md`, `targets.md`, `catalogue.md`, `runner.md` | the hand-written guides | docs test validates every `# isoloom.yml` block + every internal link |
| `ARCHITECTURE.md`, `docs/adr/*` | architecture + decisions | hand-written |
| `.github/CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` | contributor/community | audited below |
| crate rustdoc | the public library API | `RUSTDOCFLAGS=-Dwarnings cargo doc` gate (green) |
| Isothrum repo README + events schema | the runner, compiler, WASM, CLI | Phase 7, isothrum@4ae945d |
| launcher README | the three modes, the Lab engine | Phase 7, cyber-ctf-launcher@c49b34a |
| `@isothrum/editor` README | the shared editor package | verified accurate, no change needed |
| cyberctf-web / cyber-courses-web content | per-objective submission, lab/course flow | Phase 7 website worker (local main only, not pushed) |

## Generated-doc facts — enforced, verified current

All generated artifacts were confirmed current by running their stale/schema tests (all green):
`docs/spec.md`, `docs/matrix.md`, `schemas/spec-v1.schema.json`,
`schemas/run-manifest-v0.schema.json`, the catalogue schemas. No regeneration commit was needed —
earlier workers kept them current.

This covers, by construction, every docs-fact that is a spec field, allowed value, limit, problem
code or target decision, e.g.:

- Spec syntax facts (Phase 2 "the spec front end": `os`, `app:`, `service:`, ports map, `files:`,
  `overrides.<target>`, `setup:`, `environment` inputs, `hardware`, `command`, the refused old
  keys) → `docs/spec.md` + the schema. Also narrated in `docs/getting-started.md` and the README.
- Phase 4.1 step kinds (`repository`, `file`, `template`, `user`, `group`, `service`, `command`,
  `firewall`) and their validation → `docs/spec.md` + the schema; the catalogue side in
  `docs/catalogue.md`.
- Problem codes, exit codes, the `✗ error[<code>]` format (Phase 1c "import, the command line")
  → `docs/spec.md` reference + ADR 0003 + `isoloom --help`.
- Every target's per-field decision, refusal step and reason, and the tiers (Phase 6b) →
  `docs/matrix.md`, narrated in `docs/targets.md`.
- Run-manifest fields, placeholders, port names, seal, slots (Phase 2/3, cloud) →
  `schemas/run-manifest-v0.schema.json` + `docs/runner.md` + ADR 0004.

## Fact-section reconciliation

### From the website sync / docs-truth unit (top of the log)
These recorded *old* site pages that overclaimed (Terraform/Pulumi "runs on", stale VirtualBox/
QEMU/docker-vm run claims, cloud-vm limits). **Obsolete as written** — those pages are replaced.
The truthful successor is `docs/matrix.md` + `docs/targets.md`, where every claim is the
generator's decision or recorded tier evidence. The cloud/VM "proven vs generated" honesty is in
`docs/targets.md` → *What is proven at 0.11.0*. **Recommend striking** these pre-rebuild lines
from the log; superseded.

### Real runs recorded 2026-10-14 / 2026-10-15 / Phase 2 real-run lines
Evidence, not doc copy. Recorded in `tiers.json` and surfaced in `docs/matrix.md`. Documented.

### Phase 1a subtract
- `isoloom coverage`/`COVERAGE.md` gone, matrix is the replacement → ADR 0005 + `docs/targets.md`.
- `isoloom docs --spec` lists schema fields only → `docs/spec.md` header + `docs/getting-started.md`.
- Import notes, no committed `.isoloom/`, no compatibility forms, OS `<family>@<version>`,
  refused old keys → `docs/spec.md` + README (the "no `docker:`/`vm:`" bullet) + ADR 0001.
- "The hand-written docs are gone until Phase 7" → **done**: this phase wrote them.
- "Stale pointers left on purpose for later units" (README lifecycle commands, `docs/json.md`,
  `docs/real-targets.md`, `CHANGELOG`) → **resolved**: README rewritten (no lifecycle commands),
  CONTRIBUTING rewritten. **Recommend striking** these bookkeeping lines.

### Phase 1b structure / Phase 1c tooling / test-host mechanics
Developer-process facts (crate layout, `cargo xtask` tasks, the ratchet, lint set, deny.toml,
nextest/msrv/features/fuzz/mutants/bench, check-all host mechanics, SSH mirrors, clock handling).
**Reflected in** `.github/CONTRIBUTING.md` (the gate list, the ratchet, the layout, the lint
policy), `ARCHITECTURE.md` (crate split, one-way deps) and ADR 0002, plus the `xtask` crate's own
module docs. Not user-facing product docs by design. The one user-visible change here — the
install script moved to `packaging/install.sh` — is in README + `docs/getting-started.md`.
- "README's crate table still names `isoloom-external`", "CONTRIBUTING says `crates/*/src` /
  `code_size.rs`" → **resolved** this phase (README crate table now lists `isoloom-ansible`,
  `isoloom-manifest`, `isoloom-catalogue`; CONTRIBUTING updated to the grouped layout and
  `cargo xtask quality`). **Recommend striking** these two stale-pointer lines.

### Phase 2 runner skeleton / launcher / compiler / WASM (Isothrum + launcher + website)
- Runner commands, options, home, lifecycle rules, events v0, error codes, secrets, slot defaults
  → **Isothrum repo README** (isothrum@4ae945d) + the committed `events-v0.schema.json`; the
  Isoloom side (what the manifest carries) is in `docs/runner.md`.
- Launcher dependency on layered crates, the `CYBERCTF_RUNNER` switch, the Lab engine setting,
  lab errors, ports by name, attack box, progress → **launcher README** (cyber-ctf-launcher@c49b34a).
- Compiler/WASM/editor facts, `choices_json()`, starter scripts, OS list, port names →
  Isothrum README + `@isothrum/editor` README (verified) + isothrum.com (website worker).
- `isothrum.com` / `isoloom.com` sync facts (the removed `isoloom coverage --json` sync source,
  export-panel `isoloom down`) → website repos; the new docs source is `docs/spec.md` +
  `docs/matrix.md` + the `isoloom` commands (noted in `docs/targets.md`/README). Handed to the
  website worker; see "Owed / watch".

### Phase 3 (vagrant, server, cloud, docker-vm) and Cloud run manifests
- Vagrant/Proxmox/ESXi/docker-vm manifest shapes, providers, lifecycle, recovery, secrets,
  relays, the `node` requirement, the seal, `ssh.run wait_s`, `terraform.copy_module`, the
  self-setup path → `docs/runner.md` (manifest + seal, cross-referencing ADR 0004) and
  `docs/targets.md` (providers/clouds, tiers). The driver-level detail is in the Isothrum README.
- Launcher server/ESXi-through-the-runner → launcher README.

### Phase 4 (catalogue, CM model, renderers, cloud-init, Ansible) and Phase 5 (verify, examples)
- `service:`, `with:` typing, release matching, entry forms/refusals, `resolved.json` services,
  the step kinds on each form, the configure/shell/PowerShell/Ansible/cloud-init renderers,
  conformance, Docker pinning, published-ports masquerade → `docs/spec.md` + the schema (fields),
  `docs/catalogue.md` (using/writing an entry, `refresh`, `check`), `docs/targets.md` (the forms
  per target). nginx is the only shipped entry — stated plainly in `docs/catalogue.md`.
- `verify` baseline to zero, embedded shellcheck, example files, Azure VM sizes, OCI tags →
  conformance/validator facts; reflected in `docs/targets.md` (honest proof) and CONTRIBUTING
  (the `verify` gate). Validator mechanics are developer-process.

### Phase 6 (tf-test, combination fuzzing, tiers, security remediation)
- `tf-test`, `fuzz-combos`, `mutants` catalogue, idempotent `tiers record` → developer-process,
  in CONTRIBUTING / xtask docs.
- Security fixes C1/S1/S2/S5/C2 → the user-visible one (privileged container on `internet:false`
  refused) is a validation rule in `docs/spec.md`/the schema and ADR 0004. The Docker-from-apt-repo
  install and the owed RPM/dnf path are in "Owed" below.

### Crates.io publishing / isothrum-ui split
- All thirteen crates publish at one workspace version, Apache-2.0; tag `vX.Y.Z` publishes via
  Trusted Publishing → `README.md` (crates.io, versioning), `PUBLISHING.md`, ADR 0002. Isothrum's
  RC version strings corrected to v0.11.0 this phase.
- `@isothrum/editor` package → its README (verified) + launcher/Isothrum notes.

### The two websites' content facts
Per-objective submission, MDX sanitiser, content-org-only fetch, capstone `invoice-portal-sqli`,
course-references-lab-by-slug, CyberBackend validations → owned by the website worker (local main,
not pushed). See "Owed / watch".

### `push: 2026-10-16 …` log line
Not a doc fact — a deploy record. Leave as history or strike; not part of the doc surface.

## Owed / watch (documented as owed — not overclaimed)

These are real and are stated honestly in the docs rather than hidden:

1. **Cloud mode is generate+validate-proven, not real-run proven.** Stated in
   `docs/targets.md` and the launcher README. No doc claims a real cloud run.
2. **Server mode (Proxmox/ESXi) is Tier 2** pending Linux/Windows host runs; macOS real runs
   recorded. Stated in `docs/targets.md`.
3. **C1 RPM/dnf Docker path** still uses Docker's install script (apt uses the pinned repo). A
   1.0 follow-up; mentioned as a known detail, not user-facing doc copy. **Owed to maintainer.**
4. **C3/C4** Proxmox/ESXi base-image pinning to dated releases + sha256 — owed, needs network
   access. **Owed to maintainer** (not an RC blocker).
5. **Website docs** (cyberctf-web, cyber-courses-web) are aligned on local main only and **must not
   be pushed** until the maintainer approves the 1.0 release (a push deploys production). The
   isoloom.com / isothrum.com docs-sync source must be repointed from the removed
   `isoloom coverage --json` to `docs/spec.md` + `docs/matrix.md` (or `isoloom` commands) when the
   sites are rebuilt — this is Phase 7's "rewrite from the new docs" step, owned at release time.

## Still unreconciled

None blocking. The only facts not turned into user-doc prose are developer-process facts, which by
design live in CONTRIBUTING, ADRs, ARCHITECTURE and crate docs rather than the product docs.

## Recommended strikes from `plan/docs-facts.md`

Obsolete bookkeeping now resolved (safe to remove when the log is next tidied): the pre-rebuild
"website sync" / "docs-truth unit" lines; every "stale pointers left on purpose" line (Phase 1a,
1b — README/CONTRIBUTING now fixed); and the `push: 2026-10-16` deploy record. Left in place here
to avoid racing concurrent writers on that file.
