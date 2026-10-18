# Isoloom: the plan to a clean 1.0

Working document for the local rebuild. It is squashed away with the history at the first satisfying version.

## Status (updated 2026-10-15)

| Item | State |
|---|---|
| **Phase 0: safety net** | **Done.** 336 characterization snapshots (`d64255fb`). `verify` with the pinned validator image and 886 baseline failures (`115bb503`). `dev-link`, and `check-all` on macOS, Linux (`jarvis`) and Windows (`karen`) (`ac79b0c6`). |
| **Phase 1a: subtract** | **Done** (`58ae8e6b`). About 195k lines and 943 files removed: coverage, committed outputs, compatibility, hand-written docs, the CHANGELOG. Tests are green on macOS. |
| **Phase 1b: runtime out, layout, xtask, ratchet** | **Done** (`fd001ce6`): layout, runtime out (last at `b6c218c9`), backends independent, `cargo xtask`, the dependency-direction test, and the 1c lint set with a 2,549-finding baseline (terraform 613, spec 475, render 306, targets 247, plan 164, kubernetes 148, vagrant 144, compose 134, cli 119, import 100, manifest 95, ansible 3). The Windows and Linux legs are pending (hosts offline). |
| **Phase 1c: deep organization, zero violations** | **DONE.** All 13 Isoloom crates at 0 findings with closed review checklists (`isoloom-spec` keeps 2 reasoned `#[expect]`s on the lowered `Machine`/`Port` booleans). Mutation testing closed on spec, manifest, plan and render (equivalent mutants listed with reasons); fuzz clean. **Isothrum Phase 1c DONE** (`4c03a3a`): runner, cli, compiler and wasm at 0; every lint at deny; mutants closed on the compiler and the runner's executor/readers; fuzz clean (drawing 4M runs). Open: the wasm32 build isn't verified on this Mac (the target isn't installed). |
| **Phase 2: walking skeleton** | **Done** on `main`, including follow-ups: the spec switch (`e9a9dd66`), the contract fixes (`7b93fef6`), the runner on the new manifest following each slot's `default` (`isothrum@5ec348c`), and the launcher on the new syntax behind the hidden `CYBERCTF_RUNNER=isothrum` switch (`cyber-ctf-launcher@6449daa`). Real runs 6/6. The hand-rewritten `invoice-portal-api` lives privately in `~/code/isoloom-corpus/rewritten/` until the CTF lab rewrite (Phase 7). |
| **Phase 3: launcher modes** | **Vagrant:** run manifest (`17c871da`: Vagrant ≥ 2.4.0, box architecture) and runner driver for every local provider, including Hyper-V elevation (`isothrum@8b369a6`); park_controller (`47106cdd`); real runs on Mac VirtualBox, friday libvirt (full cycle) and karen Hyper-V (full cycle, Tier 1; netsh portproxy, the controller hostname and the suspend-list fixes, `149d4666`); the launcher's VM mode through the runner (`efb13a6`, Lab engine setting, Mac VirtualBox through the launcher passed). **Server mode done:** Proxmox through the runner and the launcher's ServerRun (6 real-server bugs fixed test-first; `ea217edb`, `isothrum@d714009`, `cyber-ctf-launcher@b4ba5c5`); ESXi through the runner and **the launcher** (`f52860ce`, `isothrum@000f08c`, `cyber-ctf-launcher@06db912`, tier record `df281d82`): connection secrets for every operation, a fixed relay session leak that exhausted ESXi's logins, and `park_controller` treating ESXi's already-off as halted. **Still on the launcher's old runtime:** every lab while the Lab engine setting is Built-in (the default); docker-vm (Docker on a local VM, a remote host or a server) and the clouds (cloud VMs, cloud Docker, cloud services); the attack VM and attack box; the server self-test. **Open proofs:** a real Windows-guest lab, and a real UAC prompt (karen has UAC off). Also on the old runtime: the clouds (their project archive and guest SSH steps aren't runner steps yet); the Terraform `esxi` target's guest SSH (Tier 2); file inputs on Proxmox VMs; ESXi in the launcher (the mapping on `VagrantRun` comes after the Vagrant launcher step). Everything is behind `CYBERCTF_RUNNER=isothrum`. |
| **Phase 4: catalogue and CM model** | Design approved. **4.1 done** (`a524ebd5`). **4.3 done** (`ab7d4463`): `isoloom-catalogue` (embedded entries, release matching, typed `with:` settings, `when`/`for_each`, the daemon table, schemas), `service:` lowering with coded refusals, `cargo xtask catalogue refresh` and `catalogue check`, and nginx's entry with a real `lock.yml` (1.30.6/1.31.7). nginx's real parity runs move to 4.6. **4.2 renderers landing** (the recipe carrier `5ad734eb` is in). Decisions: a `with:` setting with neither `required` nor `default` is optional (the nginx example needs it); `alpine` alone stays 3.19 for now; `isoloom-catalogue` joins `cargo xtask mutants`' default list and its run completes in Phase 6. Open for 4.7: `image.start: run` (redis), the password `owner` hand-off, and the service origin in the run manifest (a cross-repo contract change). Follow-through (4.9): the CLI prints `Lab::warnings()` in `validate`. |
| **Phase 5: validator baseline** | **5a done** (`16cd2692`): 896 → 344 entries. shellcheck, kustomize, hadolint and helm-template at 0; examples ship their files; verify is CPU-capped. **5b running:** tflint (272) and cloud-init (70) to 0, `corp-ad-basics`' missing files, and shellcheck for embedded shell (Vagrantfiles, heredocs, cloud-init). |
| **Phase 6: hardening** | **6b done** (`e2c94630`): `isoloom matrix` (96 rows × 24 columns; every cell is the generator's own decision; honesty tests; `docs/matrix.md` generated) and tiers worked out from recorded runs (`tiers.json`, `cargo xtask tiers record`). **Tier 1: `vagrant/libvirt`** (friday), **`vagrant/hyperv`** (karen, `042925e9`; UAC is off on karen, so the elevation prompt itself is unproven) **and `docker`** (`e25e5f96`: Mac, jarvis and karen, three labs each through the full cycle; the flag-file proof passed on Linux and Windows). Tier 2 with Linux/Windows runs pending: vagrant/virtualbox (Mac), proxmox and vagrant/vmware_esxi (Mac against the lab servers). **6a done** (`906f93e0`): `cargo xtask tf-test` (37 suites, 158 runs; all six clouds, cloud-docker, cloud-services, Proxmox and ESXi on mocked providers; `--fast` in check-all, the full set in `--deep`); 19 module bugs recorded as expected failures (the list is under Phase 3). Still to do: combinations (10k random specs), the security review, and making the CLI's tier test use a fixture instead of the real `tiers.json` (it hard-codes docker's tier and breaks on every new record). |
| **Isothrum on the new syntax** | Compiler, WASM (`cargo xtask wasm --website`, npm API unchanged plus `choices_json()`) and desktop app write and read the new syntax (`isothrum@4ed9069`). Compiled drawings pass Isoloom's `Lab::problems()` (property-tested); the crates are at zero findings. Leftovers for Isothrum's own 1c pass: `proptest` declared twice (workspace and runner). |
| **Track C: Proxmox 9.2** | Proven: a real apply, serve, check and destroy. |
| **Track C: ESXi 8.0U3e** | Proven with promiscuous mode on: nested DHCP works, the launcher's Vagrant self-test passes, and Isoloom's `esxi` Terraform module serves and passes its check (finishing from a container). Two fixes are queued for Phase 3 (the controller box on ESXi; guest SSH moving to the runner). |
| **Launcher standard (0.6)** | **Clean outside `runtime/` on Windows, macOS and Linux** (`f5f9452`), with win32 and linux baselines; 12 more lints at deny. The first Windows compile found two real bugs: a stop's Ctrl+C never reached Terraform (the deploy worker's process group ignored it; fixed), and the Hyper-V watcher took 30–90 s to start (now under 0.5 s). **e2e free of real time** (`78b59d7`): axe runs in the page (0.3–0.8 s per screen instead of 3–5 s), one test per screen, and the setup poll on Playwright's clock; 120/120 three times under load. |
| **Test hosts** | **2026-10-16 ~18:30: jarvis back (10.0.0.10, SSH ok, main Linux host, 8 jobs); karen (10.0.0.40, Windows) still unreachable (no ping/SSH) — Windows leg owed.** friday (10.0.0.30) also up for libvirt. check-all syncs mirrors to every SSH host. |
| **Websites** | Kept separate; the isoloom.com rebuild banner is live. Standard done (0 violations, Vitest, typed scripts). **isothrum.com's editor writes the new syntax** with exact parity to Isoloom (`isothrum-website@7fdc123`, local, not pushed). Leftover: the export panel still says `isoloom down` (a removed command); fix it with the runner's command at the docs gate. |

**Queued for when jarvis and karen are back:**
- `check-all --hosts linux,windows` on every repository;
- the Linux host setup script (Rust user-level tools, Terraform, kind, Helm, Ansible, pwsh) and the Windows host setup script (Vagrant, Terraform, pwsh);
- the launcher's `pnpm quality --update` on both, for the Windows and Linux baselines;
- the flag-file proof on Linux and Windows Docker (owner, mode, no flag in `env`);
- the runner's real run on jarvis;
- the Windows leg of Phase 1b's final run;
- **first:** the launcher's Windows-only Rust code moved during the cleanup has never been compiled (`platform::sys::windows`, the elevate split, job objects, registry, winget ids). Run Windows `pnpm check` and clippy on karen. Once that's clean, raise the launcher's zero-count lints from warn to deny.

## Resume point (usage limits hit Fri 2026-10-16: session limit about 10:45, then the **weekly limit**. It resets **Sun 2026-10-18 at 20:00 Paris**. Restart with `/loop continue until the plan is finished`.)

Workers stopped mid-step; their state is safe in their worktrees. Resume each by id with SendMessage, which keeps its context. Start a fresh worker from the worktree only if a resume is refused.

**Maintainer decisions taken 2026-10-16, before the stop. Apply them on resume:**
- **CyberBackend:** "Tests + security fixes". Workers may change `~/code/cyberbackend` on its local main only, never pushed or deployed. Allowed:
  - the flag-check tests, `isObjectiveKey`, the leak test and `test/e2e/server.ts`;
  - validating `contentRepo`/`contentPath`/`contentRef` in syncActivities;
  - counting only a learner's first correct attempt per activity;
  - refusing a sync that overwrites an activity whose stored contentRepo differs.

  **Not yet approved:** the AMOUNT decimal-compare fix (ask), and a separate `courses:sync` scope in Cyber Auth (ask).
- **Course grading:** "Embed the CTF form". cyber-courses-web gets the per-objective form, `submitObjective(labId, key, answer)` with the implicit key `main`, `lab.objectives` and `myProgress`, reusing cyberctf-web's code.
- **SQLi capstone:** "Build invoice-portal-sqli". Build it as a real CyberCTF lab: an invoice portal with a MySQL injection, and the analyst's credentials as objectives. **Done (2026-10-16):** `~/code/invoice-portal-sqli` (`10b5c30`, local, no remote, never pushed). Flask + MySQL 8, one UNION-able search endpoint, two objectives (`find-injection` token, `analyst-credentials`). Per-player flags delivered as 0400 files, never env; offline start uses placeholders. Proven: `isoloom validate`/generate/check green, an offline and a delivery run through the Isothrum runner (solver recovered both values, `docker inspect` shows no flag anywhere), 7 pytest. Objectives live in `.ctf/objectives.json` (JSON, which publish-lab-action reads; not `.yml`). Lesson pointer fixed (`cyber-course-sqli@de6751c`). The course.json lab block is migrated to the slug-only reference (`cyber-course-sqli@b9622f1`, not pushed): dry-run accepts it, a published lab syncs 26 activities, and an unpublished lab is refused fail-closed. Findings 1 (course half) and 3 closed. No GitHub repo and the production placeholder DRAFT stay for the maintainer.
  - **Where:** in a new private local repo (`~/code/invoice-portal-sqli`), git init'ed with local main. No GitHub repo is created without the maintainer's OK.
  - **Isoloom spec:** the new syntax.
  - **Course pointers:** the lesson's `<Lab slug>` and the course's lab reference both point at it.
  - **Production:** the placeholder DRAFT lab in production stays until an admin action, which needs the maintainer.

| Worker (id) | Worktree / branch | State at stop | Next step |
|---|---|---|---|
| Phase 4.2 renderers (`ab16abd1636a84517`) | — (remove `isoloom-4.2-render`) | **done** (`1acdbe8c` renderers, `69e311bc` the runner for `image.start: run`, `84155a47` the systemd `WorkingDirectory` fix found on a real VirtualBox run; conformance 8/8, `vagrant/virtualbox` tier evidence) | **4.4 cloud-init landed** (`isoloom@885b870c`: narrow parity-safe renderer, recipe compressed in write_files + runcmd, Proxmox wired, conformance proven). Next: **4.5 Ansible** (match the shell library's guards; reuse the conformance renderer axis; lift `carry::on_vm(Host::Existing)`) and **4.6 conformance** (nginx parity). Follow-up: on VMs a recipe's `start` steps run once at provisioning; the design wants a boot unit before the services |
| Phase 3 docker-vm (`aec8376e1618bb59f`) | `isoloom-dvm` (`tmp/dvm`), `isoloom-dvm-manifest` (`tmp/dvm-shared`, the shared guest commit `7f125358` in gates), Isothrum `isothrum-wt/dvm`/`dvm2`, launcher `cyber-ctf-launcher-wt/dvm` | was persisting the VBoxVGA controller in the docker-vm Vagrantfile and a published-port masquerade on Docker host VMs | land the shared commit (`Placed`, `run_manifests`, `run_manifest::guest`, `isoloom_compose::guest`, setup.sh **with a pinned Docker version**, guarded clear); send the SHA to the cloud worker; then docker-vm. Rebase the launcher worktree on `4ae15b1` |
| Phase 3 clouds (`a781d690309c173fb`) + runner helper (`a0cb135bbe4435ce4`) + cloud launcher (`a0a56435d62ac83c0`) | `isoloom-cloud`, `isoloom-cloud2-pin`, Isothrum `isothrum-wt/cloud2` (`tmp/applyfix`), launcher `cyber-ctf-launcher-wt/cloud` | runner: `wait_s`, sensitive outputs masked (`isothrum@40eb4c5`); launcher import landed (`36088b9`); seal design approved (`Option<Seal>`) | rebase on 5b `edb0d9c4`; adopt the shared guest commit once landed; fix the two gaps the launcher worker found: `Runner::adopt` must accept an `absent` record with no state, and cloud-services `run.json` needs a `region` param replayed at destroy | — **manifest contract landed** `isoloom@74e8972e` (seal, ssh.run wait_s≤1800, terraform.copy_module; others byte-identical). Cloud modules (cloud-vm, cloud-docker on GuestCompose, cloud-services+region, the egress seal on all six incl. Azure vnet-deny, OCI/DO/Linode SSH secrets) staged on `tmp/cloud`, generate + terraform-validate green; NOT landed — the ~30 `tests/terraform/cloud-*` suites need rewriting for the self-setup module shapes before landing — **DONE 2026-10-16** `isoloom@c1137fb5` (cloud-vm/docker/services on all 6 clouds + egress seal; tf-test rewritten; **10 internet:false known_bugs flipped to enforced** — aws2/azure3/gcp2/linode1/oci2; 6 cloud-docker `published` known_bugs dropped→run-manifest output; gates green incl verify 0, tf-test 0 failed, 2008 tests). Generate+validate only, no real cloud run (needs AWS creds). Remote check-all legs owed.
| Phase 5b verify (`a8fa25ccb1210df50`) | — | **done** (`edb0d9c4`: baseline 344 → 0) | the Windows leg is owed (karen down) |
| cyberctf-web standard (`a7c75d5880c99fac2`) | — (landed) | **done** (`cyberctf-web@131ee17`, not pushed): ratchet zero (3 reasoned disables), 405 unit + 42 Playwright green, coverage 74.7% (backend/dashboard 100%), built HTML unchanged. The objective form is a self-contained module (`lib/objectives` + `components/objectives`); `lib/server/cyberbackend` → `lib/backend/*` + `lib/auth/*`. Flag e2e kept opt-in, imports moved | — |
| Flag contract (`a573c63cf2a7ee092`) | — (landed) | **complete, all proven green** | **done.** launcher `launch_env` passes `CTF_FLAG_*` only (`8b34643`); `real_flag_labs` harness (`9d20f39`) runs green end to end on both engines; CyberBackend `isObjectiveKey`+property test, leak test, `test/e2e/server.ts` (`7fc7f38`, 172 tests); Isothrum runner leak tests (`75c20f6`); cyberctf-web opt-in e2e (`131ee17`); Isoloom CLI secrets test (`5542d1b2`). **AMOUNT decimal fix: proposal only** (`scratchpad/flag-amount-proposal.md`), awaiting the maintainer |
| cyber-courses-web (`af28428e203e61834`) | main checkout (alone) | landed so far (not pushed): content-fetch hotfix (`099adc6`, also `hotfix/content-fetch`), PR #6 animation lib (`b982acd`), quality ratchet + package rename + single lockfile (`0380d85`), four drift items (`0d13d7f`), embedded per-objective form copied from cyberctf-web + zod-typed lab boundary (`df45fe4`) | **done** (`cyber-courses-web@7d812fb`, not pushed): consolidation, standard ratchet 333→306 (below baseline, not zero — rest is Phase-7 docs-gate work), per-objective form, 202 unit + 36 Playwright, security fixes (MDX sanitizer, image allow-list, redirect/logout hardening, lesson-route force-dynamic removed). hotfix/content-fetch preserved. docs-facts `isoloom@08f1f04f`. Dead CTF-template/markdown cluster left for the maintainer to keep (likely Phase-7 docs) or cut |
| Course contract (`a1b0a1fd8dac5460c`) | — (landed) | **sync rewrite done** (`cyber-course-sync@3fea97e`, not pushed: course.json parsed, lab by slug, old publish block refused, lesson cross-check, commit-pinned, dist==src test, 102 tests). **CyberBackend approved items done** (`cyberbackend@b62728e`, not pushed: contentRepo/Path/Ref validation, first-correct-attempt-only, cross-repo overwrite refused; 168 tests). Proposals for the held items in `ccontract-proposals.md` | **done.** cyber-courses-web opt-in e2e landed (`bf11634`, not pushed): drives the real loaders + MdxLesson against a local backend, skipped unless `CYBERCOURSES_E2E_BACKEND_URL`. Flag worker rebased and landed (`cyberbackend@7fc7f38`). Idle; held proposals in `ccontract-proposals.md` await the maintainer |
| scripts fork (`a23308a98257c826f`) | — | e2e passed (42), ports freed | — |

**Blocked by the weekly limit (Opus 4.8), resets Oct 22 00:00 Paris:**
- **docker-vm — DONE 2026-10-16** (`isoloom@690d585e`, `isothrum@3dd6005`): the runner runs the lab's containers inside a VM. Real VirtualBox run of hello-stack passed (up/check 12-0/down, clean teardown), tier `docker-vm/virtualbox` pass on mac. Mac + Linux(jarvis) gates green, 1996 tests, verify 0 new. **Windows leg owed** (karen was down at land time; it's up now — run `check-all --hosts windows`).
- **clouds (`a781d690309c173fb`)** — deep WIP in `~/code/isoloom-cloud` (`tmp/cloud`, `619a6d81 wip4`, 32 dirty): cloud-vm, cloud-docker on GuestCompose, cloud-services+region, the egress seal on all six clouds (Azure vnet-deny, OCI/DO/Linode SSH secrets). Generates + terraform-validates. Left: rewrite ~30 `tests/terraform/cloud-*` suites for self-setup machines (drop the ready_file/provisioner assertions, flip the internet:false known_bugs), then land against Isothrum `07e07fc`.

**Phase 6 security review done (2026-10-16, REVIEW ONLY; full report `scratchpad/p6sec-report.md`).** Verdict: strong — quoting centralised + proptested + fuzzed across all 5 languages, secrets/flags never in generated files (references only; runner masks + delivers over SSH stdin), sensitive input refused in `environment:`, seal fail-closed, path traversal confined, images digest-pinned, zero `unsafe`, zero `set_var`, SSH exec argv with exact-name SendEnv/AcceptEnv. Findings routed:
- **C1 HIGH** unpinned `get.docker.com` installer → security worker (pin via Docker repo + key + version, the TODO(4.7) fix).
- **C2 MED** Galaxy collections installed without the pinned sha256 → 4.5/4.6 Ansible worker.
- **C3/C4 MED** Proxmox/ESXi cloud images on floating `latest`/`current` URLs, no checksum → cloud worker.
- **S1 MED → DECIDED:** `internet:false` on local docker/docker-vm was only guest-enforced; a privileged/NET_ADMIN container could egress. **Fix: refuse privileged/NET_ADMIN/NET_RAW on `internet:false`/offline-network machines** (fail-closed, no silent opt-out) → security worker. Makes internet:false a real local guarantee, matching the cloud seal.
- **S2 LOW/MED** file `mode` unquoted in chmod (lib-consumer hole, not CLI-reachable) → security worker (quote / `Mode` newtype), matters now Isoloom ships as a crate.
- **S3 LOW** Isothrum dotenv key not escaped → security worker.
- **S4** resume replays sealed=true via apply (acceptable) — add a test.
- **S5 by-design** (TF_VAR via `-e` visible to docker-daemon holders; `ANSIBLE_HOST_KEY_CHECKING=False`; `proxmox_insecure` TLS opt-out) — documented as conscious trade-offs for ephemeral labs; a code comment added at each.

**Release plan (decided 2026-10-16): tag v0.11.0 (forward RC; 0.9.0 rejected — Isoloom already at 0.10.1), 1.0.0 after docs+labs.** No squash (tag on current history). crates.io via **Trusted Publishing** (OIDC, maintainer configures Trusted Publishers per crate; first publish of a new crate name may need one manual `cargo publish`). Desktop: **unsigned draft** release. Both held CyberBackend/Auth changes **DONE** (local main, not pushed/deployed): AMOUNT decimal fix `cyberbackend@d2f1ff8`; `courses:sync` least-privilege scope `cyberbackend@969ad6f` + `cyber-auth@392d2ea`. To use courses:sync: an admin creates a content-repo machine client with that scope in Cyber Auth and adds its id to CyberBackend's `COURSE_SYNC_CLIENT_IDS`.
**Gates before the v0.9.0 tag:** (1) clouds land; (2) C1 Docker-install pin (option a: apt repo-based + pinned key, get.docker.com kept for RPM with a 1.0 follow-up) lands after clouds; (3) AMOUNT + courses:sync land; (4) a clean full check-all (mac+jarvis+karen) on the final main; (5) workspace version bump to 0.9.0; (6) tag.
**Desktop draft dependency:** release.yml checks out the PRIVATE `isoloom/isothrum-website` for the editor sources, so a CI draft needs `WEBSITE_REPO_TOKEN` (read-only) OR the website repo made public. Local build proved a mac dmg unsigned (`Isothrum_0.1.0_x64.dmg`, 10.4 MiB); win/linux need the CI run.

**C3/C4 (owed, needs network access):** pin the Proxmox and ESXi base OS images to a dated release + embedded sha256 in the image table (like the catalogue lock), replacing the floating `latest`/`current` URLs. Server-target generators (part of the Proxmox/ESXi server follow-up), not blocking the RC.

**Phase 6 security remediation COMPLETE (2026-10-16).** Landed test-first on main: C1 `isoloom@bd48a798` (apt-repo Docker install, key pinned by sha256, madison version resolution, get.docker.com only the RPM fallback, fails closed), S1 `57928f91`, S2 `47918dba`, S5 `1a942452`; isothrum S3 `6a86253`, S5 `6c096ad`; C2 `7158a3ca`. All gates green (verify 0, full nextest, 44 Docker-only snapshot regens). **Owed before 1.0 (not RC blockers):** (a) inline C1's dnf/RPM repo path with its pinned key `e6c650e0…`, proven on a Rocky/Fedora guest (TODO 4.7); (b) a real apt-guest run on jarvis to exercise the new Docker bootstrap. **C3/C4** (Proxmox/ESXi image pinning) still owed to the maintainer (needs network access).

**isothrum-ui split DONE (2026-10-16, local-only).** The shared lab editor is extracted to a new package **`@isothrum/editor`** (name confirmed by the maintainer) at `~/code/isothrum-ui` (`07be470`, Apache-2.0, no remote). Both consumers use it via `file:` link: website `fe36907` (auth/branding injected as ports, stays private) and desktop `isothrum@2ea6621` (release.yml dropped the `isothrum-website` checkout + `WEBSITE_REPO_TOKEN`). Builds green (pkg 342 tests, website next build + 248, desktop cargo tauri build). **Maintainer publish steps (gated):** create GitHub `isoloom/isothrum-ui` (public, Apache-2.0) + push; create/confirm the `@isothrum` npm org + `npm login`; `npm publish --access public`; then flip both consumers from the `file:` link to `^0.1.0`. This removes the desktop build's private-repo dependency entirely — the earlier WEBSITE_REPO_TOKEN question is now moot.

**Phase 7 docs DONE, alignment gate PASS (2026-10-17).** Generated docs confirmed current (`docs/spec.md`, `docs/matrix.md` = the generated coverage; `COVERAGE.md`/`docs/catalogue.md` don't exist as artifacts — coverage was removed in Phase 1a, matrix.md replaced it; catalogue.md is now a hand guide). READMEs + CONTRIBUTING + `docs/{getting-started,targets,catalogue,runner}.md` + ARCHITECTURE.md + `docs/adr/0001-0005` landed and pushed: isoloom `d4946118`, isothrum `4ae945d` (+ added Apache-2.0 LICENSE), launcher `c49b34a`. `plan/docs-alignment.md` maps every docs-fact → where documented or "owed". **Website docs landed LOCAL only, NOT pushed** (a push deploys prod): cyberctf-web `cc62cb0`, cyber-courses-web `48f1a93` — push on the maintainer's OK. **At release:** repoint isoloom.com/isothrum.com docs-sync from the removed `isoloom coverage --json` to `docs/spec.md` + `docs/matrix.md`.

**Real-AWS proof DONE (2026-10-17) — a 1.0 milestone.** All three cloud modes ran a full real cycle on the account (us-east-1, t3.micro), torn down with ZERO leftovers (AWS CLI verified: instances, vpcs, sgs, subnets, eips, nics, key pairs, volumes, s3 all empty): cloud-vm (solo-web up/check/down/reap + segmented for the seal proof), cloud-docker (Docker-in-VM over SSH, web 200), cloud-services (S3 site 200). **The egress seal is proven LIVE:** the sealed internet:false machine had no outbound internet while SSH + reach rules worked (second `terraform apply` drops the 0.0.0.0/0 route + limits SG egress to lab CIDRs). No cred value hit a log/file (ambient default profile, runner re-injects only AWS_PROFILE/AWS_REGION). Bug fixed + landed `isoloom@c7ecfec1` (cloud-docker first SSH needs `wait_s=300` for the booting VM). Tiers `846796bf`: cloud-vm/aws, cloud-docker/aws, cloud-services/aws = pass on mac (tier 2; Linux/Windows real runs still pending for tier 1). **Other clouds (Azure/GCP/DO/Linode/OCI) remain generate+tf-test+validate only — one command each with that cloud's creds.** Minor follow-up: `into_vm`'s first step may want `wait_s` generally (docker-vm worker heads-up).

**CTF lab-rewrite pilot DONE (2026-10-17, corpus local-only `isoloom-corpus@2ab9fb0`, not pushed).** Tool `tools/rewrite.py` converts old→new syntax (ports map, flag→`files:[{input}]`, build/image/vm/overrides/message rewrites). Pilot 18 labs: 17/18 validate; 3 real Docker cycles clean (flag as file `/ctf/flag` 0444, never env; seal `no internet` check passed; teardown clean). **Classification of all 376: 292 full / 55 todos / 29 manual / 0 errors.** **BATCH DONE (2026-10-17): all 292 full rewritten + validated 292/292** **ALL 376 rewritten (2026-10-17, `isoloom-corpus@859a916`, local): 292 full / 84 todos / 0 manual / 0 blocked.** The 29 sealed-privileged jails validate (29/29; 3 proven real-run with `seal: best-effort`); rewrite.py auto-adds the opt-in. **Remaining lab work (a review pass, not blocking):** the 84 todos' flag scripts read `$CTF_FLAG_MAIN` from env but flags are now files (`/run/lab/main`) — repoint each script to read the delivered file; + upstream-source restore for build-from-folder labs before real runs. (`isoloom-corpus@4de94d8`, local; tool bug fixed: GOAD group `vm:` blocks). 9 image-based labs real-run clean on Docker (checks + `no internet` seal + flag-as-file). Remaining: 55 todos (flag read-path review), 29 sealed-privileged (blocked on the privileged opt-in — resume the lab worker when it lands), and upstream-source restore for build-from-folder labs before their real runs. Scaling: transformation is seconds/lab; the cost is verification + restoring trimmed upstream source (409 build-from-folder labs need their pinned `branch@commit` source; 70 image-based run as-is). Next: batch-rewrite the 292 full + validate; hand the 55 todos (flag read-path/DB-seed scripts) to reviewers.
**DECISION NEEDED (reverses part of S1):** the validator refuses `privileged:true` on an `internet:false` network, blocking **30 legit sealed-privileged labs** (Google CTF pwn, cicd-goat — privileged AND isolated is the point). Proposed 1.0 fix: an explicit opt-in (privileged-on-sealed allowed only with an acknowledgement flag recording the seal is best-effort for that machine), safe-by-default. Awaiting the maintainer's go to implement.

**Lab engine-compatibility gating (decided 2026-10-17, implementing).** Per-lab compat record in CyberBackend: `specFormat` (legacy|v1), `minIsoloom`, `minIsothrum` — computed at publish/sync as the engine versions the lab was validated against; the launcher reports its bundled Isoloom+Isothrum versions and the catalogue returns ONLY labs where format=v1 and bundled>=min for both; incompatible labs are hidden; un-synced labs default to legacy (hidden until re-synced). Spans CyberBackend (fields+filter), publish-lab-action + cyber-course-sync (compute+store), the launcher (report versions + filter), and Isoloom (`validate --json`/`compat` emitting the format+version). All LOCAL main (CyberBackend/publish-path/launcher not pushed/deployed without OK; Isoloom pushable). **Coupling:** the current launcher bundles Isoloom 0.10.1 (`00e99d16`), so it will correctly HIDE 0.11.0-validated labs until the pending launcher-on-v1 migration bumps it to 0.11.0 — expected, and it pairs this feature with that migration.

**Crates publish COMPLETE (2026-10-17 23:15 UTC).** All 17 crates live on crates.io at **0.11.0** (13 isoloom + 4 isothrum, dep order) + all 17 trusted publishers configured (OIDC `publish-crates.yml`, env `crates-io`) — future releases publish token-free from CI. Matches git `isoloom@1a77e345`/`isothrum@b0a537d` (v0.11.0). **Follow-up:** the desktop app (`apps/desktop/src-tauri`) still pins isoloom `690d585e`; it needs a coordinated bump to the published 0.11.0 on the desktop track (pairs with the launcher-on-v1 migration).
**terraform-provider-isoloom:** reserved/empty repo (LICENSE only); provider plugin dropped for 1.0 (Terraform *output* is core). Revisit in 1.x, likely as an Isothrum (lab-running) provider. Candidate for the GitHub cleanup or keep reserved — maintainer's call.

**Lab engine-compatibility gating DONE (2026-10-17).** Across 4 repos, all gates green: Isoloom `isoloom@b59a41a8` (`validate --json` emits `specFormat`+`isoloomVersion`), CyberBackend `84f327b` (Lab fields `specFormat`/`minIsoloom`/`minIsothrum` + `compatibleLabs(isoloom,isothrum)` query; legacy backfill; 197 tests), launcher `ead5991` (build.rs reads pinned engine versions → the `labs` query is now `compatibleLabs` aliased, frontend untouched), publish-lab-action `04f4ec0` (reads the format/version from `validate --json`, new required `isothrum_version` input). cyber-course-sync needs no change (links already-published labs). CyberBackend + publish-lab-action stay LOCAL (not pushed; publish-lab-action needs the `isothrum_version` input wired into each lab repo's workflow before it's pushed). **Coupling:** launcher bundles Isoloom 0.10.1 + Isothrum 0.1.0, so it hides 0.11.0 labs until the launcher-on-v1 migration.
**Squash (maintainer go 2026-10-17): confirmed, sequenced LAST.** Do NOT squash while workers are landing (force-push would orphan their rebase bases). Order: finish the in-flight wave → split apps/desktop into `isothrum-desktop` → reconcile versions (post-0.11.0 → 0.11.1, re-publish via trusted-publishing CI) → squash each repo to one commit, re-point the `v0.11.x` tags, force-push, make public, fresh GitHub releases/issues (archives in `~/code/github-archive`). **Scope to confirm:** isoloom/isothrum/launcher definitely; websites + isothrum-ui TBD. isothrum history scanned CLEAN (safe to go public).

**pve-lab UP (2026-10-17 ~02:00):** Proxmox 172.16.1.10:8006 reachable, headless. esxi-lab held (start one-at-a-time per the maintainer). Available for a real Proxmox verification of the server code-fixes (seal/hosts) + the launcher server-QA leg.
**LocalStack for the cloud LABS, not just the target paths (maintainer idea 2026-10-17, part 2) — 1.x feature.** Beyond the generated AWS target paths, the `cloud:` CTF labs that bring their own terraform (awsgoat, cloudgoat-style) could run locally against LocalStack by pointing the lab's own AWS provider at LocalStack endpoints — so students run AWS-attack labs with no account/cost. **Feasibility, honest:** awsgoat is serverless (S3 + API Gateway + DynamoDB + Lambda + IAM) → good LocalStack Community fit for DEPLOYMENT. **Caveat:** these labs are IAM-privesc challenges; LocalStack Community does NOT enforce IAM (Pro does), so a lab may deploy but not be authentically exploitable/gradeable without Pro. EC2-compute cloud labs won't boot on Community. **Plan (1.x):** a `localstack` local-sim mode/provider for `cloud:` labs + the AWS target (prove per-lab vs IAM-enforcement); prototype awsgoat-module-1 first to validate. Pairs with the OpenStack idea (local IaaS for VM-based cloud labs). NOT v1 (non-AWS clouds + local-sim ship 1.x; awsgoat runs on real AWS, proven).

**Local cloud simulation (maintainer idea 2026-10-17) — for cloud QA without real creds/cost:**
- **LocalStack** emulates AWS APIs locally → point the AWS cloud targets' Terraform at LocalStack endpoints: `cloud-services` (S3) works well; `cloud-vm`/`cloud-docker` real EC2 compute is limited on community LocalStack (pro boots more). Good for cost-free CI/QA of the AWS target paths.
- **OpenStack/DevStack** as a local IaaS that really boots VMs/networks → candidate for a **new `openstack` cloud target (1.x)** so cloud-style labs run on a local OpenStack to simulate cloud end-to-end (compute + networking + the seal) without a paid account. Not in the current 6-cloud 1.0 set.
- **Use for the v1 QA:** cover the cloud platform locally (LocalStack for the AWS paths now; OpenStack target later) instead of depending only on real Azure/GCP/DO/Linode/OCI creds.

**Catalogue 4.7/4.8 wave-1 DONE (2026-10-17, `isoloom@5fdb7250`, main GREEN).** 9 entries, each container-form conformance-proven on Docker/debian@12 through the Isothrum CLI: php-fpm, apache, openssh (build-from-OS), postgres, mysql, mariadb, mongodb (official images), juice-shop, dvwa (prebuilt). php-fpm unblocked nginx's `php` contract case. Two renderer fixes landed: a bare official-image machine keeps its own WORKDIR (juice-shop), + the red-main hotfix (3 lowering tests). **Renderer follow-ups (owed):** mongo/rabbitmq run as non-root and can't read the 0400 root-only secret mount — needs a secret-mount readable by the image's service user (postgres/mysql/mariadb read as root, fine); VM forms of php-fpm/apache/openssh + the DB entries not run here; multi-OS-family beyond debian@12. **Wave-2 follow-up:** redis/valkey, rabbitmq, wordpress, and the Windows entries iis/ad-ds (can't run here — mark planned).
**Server follow-up:** 3 bugs fixed (router seal, hosts file, ESXi progress; tf-test 0 known-bugs) on `tmp/server-fixes`; landing now that main is green, folding in the **seal live-reload** fix (the router's nftables must reload on the seal step — the second apply alone doesn't take effect on an already-booted router). Real pve-lab verification to follow (hosts-file + the now-live seal).

**ESXi QA (2026-10-17):** The **`vagrant/vmware_esxi`** target is the ESXi path through the launcher (key-auth, proven; a fresh launcher full-cycle on esxi-lab is confirming it now). The alternative **`esxi` Terraform target** has gaps and is NOT launcher-usable: (1) emits no `run.json` → no runner/launcher path; (2) uses the **password-only `josenk/esxi` provider**, incompatible with a key-hardened ESXi host (apply fails at connect); (3) its live seal reload is unwired. Static checks on it pass (hosts-file ✓, progress ✓, seal ruleset ✓). **Proposed v1 call:** ESXi ships via `vagrant/vmware_esxi`; the `esxi` Terraform target is a documented **1.x limitation** (key-auth provider + run.json + live seal). Tier: `esxi` (terraform) = partial (static); `vagrant/vmware_esxi` = pass (pending the live confirm). Maintainer can override to fix the terraform esxi target for v1.

**0.11.1 PUBLISHED (2026-10-17) — fixes BUG-2 (the premature 0.11.0).** All 13 isoloom + 4 isothrum crates live on crates.io at 0.11.1 via the OIDC trusted-publishing workflow (workflow_dispatch, no tag → no desktop-release side-effect), token-free. isoloom-spec 0.11.1 includes the `seal` field (missing from 0.11.0). isothrum zip advisories resolved by a documented deny-ignore (write-only, not reachable; `5482fa3`) + 9.x upgrade tracked for 1.x. Bumps: isoloom `96e03122`, isothrum `bfb4d8b`. **Stage B running:** re-point the launcher to 0.11.1 + prove a `seal: best-effort` lab full-cycles through the launcher = BUG-2 closed. The staged `v0.11.0` git tags (at `1a77e345`/`b0a537d`) are now superseded by 0.11.1 — drop or keep as history; a `v0.11.1` git tag is cosmetic (crates already published) and deferred to the release/squash step to avoid triggering the desktop release.

**BUG-2 CLOSED (2026-10-17).** Launcher re-pointed to 0.11.1 (`cyber-ctf-launcher@4f408b6`, pushed; compat const 0.11.1). A real sealed lab (`google-ctf-2024-pycalc`, `seal: best-effort`+privileged) ran a full cycle through BOTH engines: validated (0.11.0 rejected it), launched, 3/3 checks incl. the seal's "no internet from challenge", park/resume, clean teardown. **All three v1 bugs now resolved** (BUG-1 mysql example `cda9e661`, seal-recreate `ad45f56d`, BUG-2 via 0.11.1). **Corpus finding (routed to the lab worker, not engine):** some kctf sealed jails' `/home/user/start.sh` is mode 644 → execve denied (mytls/log4j/under-construction); restore the exec bit in the rewrite. **v1 QA status:** local 4/4 green, Proxmox green, ESXi via `vagrant/vmware_esxi` confirming. Remaining legs: cloud (AWS/LocalStack), bridged-LAN .101+, remote engines (libvirt/hyperv). Then the final squash+publish+make-public + desktop split.

**LAB-CORPUS PROOF COMPLETE (2026-10-17) — the long pole is CLOSED.** Against isoloom 2a49995a + isothrum 34244d6 (interop verified). **327/327 validate.** Track A: 28 todo flag-scripts env→file. Track B: **26/26 sealed chroot-jails pass** (no-internet seal + delivered flag reaches rootfs + clean teardown; plain jails = direct `/chroot` delivery, templated/binary = flag placement in image CMD reading `/run/lab/main`; +1 exec-bit fix on compression's `compress`). Track C: **69/69 Docker-capable build-from-folder labs restored (`CyberCTF/<lab>@sha`, ephemeral) + real-ran + pass**; 5 correctly refused non-Docker (awsgoat-1/2+cloudfoxable→AWS, dvta→Windows, kubernetes-goat→k8s). **Zero content defects remaining.** Corpus `isoloom-corpus@ee3dcb5` (local). Engine: `setup:` runs in a one-shot container of the machine's image (intended, documented `isoloom-compose/src/lib.rs:17`) — can't mutate the service rootfs; worth a doc note. **Remaining lab work:** 49 VM/AD/k8s todos proven on their platforms (not Docker).

**DECLINED (maintainer 2026-10-17: "we will not do it") — mobile client over a remote runner daemon.** Not pursued. (Kept only as a note of why the runner-only architecture would have allowed it.) The 2c-A runner-only refactor unlocks this: the Isothrum runner is now a headless engine (clean `Run`/`Running` API + JSON event stream + typed errors + attach/detach shells), so alternate front-ends are natural. Shape: (1) a **runner daemon** on the PC (WebSocket/gRPC) exposing catalogue/launch/stop/status/check/event-stream/attach — small, since the API exists; (2) a **thin mobile client** (React Native or responsive PWA) reusing the launcher web UI + isothrum-ui components; (3) a **relay** bridging phone→home-PC daemon through NAT (like Claude Code Remote Control; direct on-LAN). Reuses: runner event stream (live status), cyber-auth (auth), compat-gating (show only runnable labs), the egress seal + no-own-runtime guarantee (bounded surface). **Biggest challenge = SECURITY** (exposing an engine that runs privileged containers / spends cloud $ / touches the LAN → needs strong device auth, TLS, scoped tokens, spend/rate caps [AWS budget check helps], hardened relay). NOT v1 — net-new surface; revisit after 1.0.

**Stage 2 dedup CAVEAT (publish-time):** isothrum's xtask path-deps isoloom `crates/tools/xtask-quality` (sibling checkout) — DEV-ONLY (xtask), shippable `isothrum-runner` unaffected. So: (i) the post-release clone-build verification must build the SHIPPABLE crates (`cargo build`/`test` on the runner/cli/etc.), NOT `cargo build --workspace` which would pull the xtask + fail without a sibling isoloom; (ii) at squash/publish, convert isothrum's `xtask-quality` path dep → a git dep pinned to the isoloom 1.0.0 tag (or publish xtask-quality); (iii) document the sibling/xtask requirement in isothrum CONTRIBUTING. isoloom@e6181245 + isothrum@45e21ee pushed.

**RELEASE SPEEDUP — ONLY build parallelism (maintainer 2026-10-17: "do only 1 not the rest").** APPLIED: `CARGO_BUILD_JOBS=12` on the 16-core Mac (the free CPU win). REVERTED (full rigor kept): full gate on EVERY increment (no fast-gate shortcut), quality-tooling dedup DONE in this prep (not deferred), post-release verification runs SERIAL (not fanned out), ESXi fresh run kept serial. So only the parallelism changed; thoroughness unchanged. Est with only the jobs bump: ~3-6h autonomous.

**2c-A COMPLETE — RUNNER-ONLY MANDATE DONE (2026-10-17).** isoloom access contract `3de36b50` (on origin via fe651464), isothrum `b15aa6d` (hot-add attach/detach/shell_command + pin bumped to the access-contract isoloom, pushed), launcher `ffa71e2` (local, net −2116 LOC: attacker box = the lab's on-demand Role::Access machine via Running::attach/detach/status/shell_command; deleted exegol/attack_box/attack_vm/shell/remote; no-shell-out grep CLEAN — only the OS-terminal opener + test helpers remain). The launcher is a thin shell over the Isothrum runner — zero own lab OR attacker runtime. Proven on 6 platforms (2c Stage 3 + remote re-validation). READINESS BAR (a) MET. **NOW: RELEASE PREP running** (autonomous, per the authorized sequence) → readiness bar → squash → publish 1.0.0 → make-public → deploy → post-release verification.

**POST-RELEASE VERIFICATION — MANDATORY CLOSEOUT (maintainer 2026-10-17: "when everything is shipped make sure everything works as expected").** After the ship (publish+make-public+websites), run a full smoke-test against the PUBLISHED/PUBLIC artifacts (not the local/dev-link state) and fix anything broken before declaring v1 done: (1) **Crates** — clean temp project depending on isoloom + isothrum-runner **1.0.0 from crates.io** builds (no dev-link needed); (2) **Public-repo clone-build** — fresh `git clone` of each of the 5 public repos (isoloom/isothrum/launcher/isothrum-ui/isothrum-desktop) → full gates green with ONLY public deps; (3) **Shipped launcher** — built against published 1.0.0, runs a lab END-TO-END (docker + local VM) + the fresh ESXi run vs the published launcher; (4) **npm** — `@isothrum/editor` published version installs, website+desktop build against it; (5) **Websites** — cyberctf-web + cyber-courses-web live + docs render, isoloom.com banner gone + docs-sync correct, isothrum.com editor loads the npm WASM; (6) a corpus CTF lab full-cycles through the shipped launcher. Any breakage → **1.0.1 hotfix** + re-verify. Deliver the full verification report to the maintainer via SendUserFile. Only after this passes is v1 "done".

**RELEASE: FULLY AUTONOMOUS — NO FINAL CONFIRM (maintainer 2026-10-17: "dont wait for me, you can go when everything is ready").** This SUPERSEDES the earlier "final confirm before squash" gate AND the standing "squash needs confirmation at that moment" memory. The coordinator executes the ENTIRE release (incl. the irreversible squash + publish 1.0.0 + make-public + website deploy + announce) autonomously once the READINESS BAR is met — no check-in. **READINESS BAR (ALL must hold before any irreversible step):** (a) 2c-A fully landed — launcher 100% runner-only, no-shell-out grep clean, zero residuals; (b) all prep done — Node engines widened to 25, lab→environment rename finished, quality-tooling dedup, 1.0.0 bump + dated changelogs + isothrum README + launcher CHANGELOG, isothrum-ui GH repo + npm published + consumers flipped file:→^0.1.0, desktop cutover (apps/desktop removed from isothrum, GH isothrum-desktop created+pushed, editor flip), launcher re-pointed to published-1.0.0 deps AND verified building STANDALONE (no dev-link), CI re-enabled (ci.yml lifecycle→runner + stale usage strings fixed), pve-lab QA token + host cargo caches cleaned; (c) FULL gates GREEN on every repo (Node 25); (d) a SECRET/PRIVATE-CONTENT SCAN of each repo before it is made public (no creds/private lab content leaks); (e) all workers quiescent. **Irreversible-step ordering:** publish crates 1.0.0 (isoloom+isothrum) → re-point+verify launcher standalone → squash ALL repos → (re)publish/tag as needed → fresh ESXi run vs published launcher → make-public the 5 repos (post secret-scan) → deploy websites + banner removal + docs-sync + isothrum.com + announcement. Abort the irreversible steps + report if ANY bar item fails. Non-AWS clouds as-is (1.1). OWED USER external (NOT release blockers for the 5 OSS repos): courses:sync scope in Cyber Auth; retag publish-lab-action; NPM_TOKEN for courses-web.

**RELEASE HOLD-POINT (2026-10-18) — clean, waiting on npm re-auth.** DONE: isoloom+isothrum 1.0.0 LIVE on crates.io (core of v1 shipped — publicly installable now). Launcher re-pointed to published 1.0.0 + version fixed 1.4.1→**1.5.0** + STANDALONE build/gate verified against the shipped crates (`cyber-ctf-launcher@68ec33e` local, no dev-link, no API mismatch). SECRET-SCAN of all 5 repos CLEAN (no real creds; only AWS example placeholder keys in test fixtures; NO Password123 anywhere). All 5 repos clean at HEAD, prepped for make-public. BLOCKER: npm `whoami`=E401 → can't publish @isothrum/editor 1.0.0. DECISION: HOLD the irreversible squash + make-public + deploy until npm re-auth (coherent all-5 rollout; avoid an autonomous destructive squash/make-public while the maintainer is away; core already public via crates.io so low cost to wait). Fires immediately on `npm login`. Repos ready: isoloom 7b39dee6 / isothrum 27f0e47 / launcher 68ec33e(push needed) / isothrum-ui 49ef890 / isothrum-desktop 25308da. Minor CI note: launcher Node-25 pnpm install wants `.npmrc engine-strict=false` (transitive vitest).

**BOTH ENGINES 1.0.0 LIVE ✅ (2026-10-18):** isoloom (13 crates) + isothrum (runner/compiler/wasm/cli) all verified 1.0.0 on crates.io. **NPM BLOCKER (needs maintainer):** `npm whoami`=E401 (session expired, like AWS earlier); `@isothrum/editor` 1.0.0 dry-run clean (3.3MB tarball) but publish needs auth. Maintainer must `npm login` (or drop a token), OR confirm npm Trusted Publishing is configured for the isoloom/isothrum-ui GH repo (then publish via OIDC after the repo's created). Gates ONLY the npm tail: @isothrum/editor publish → isothrum-desktop consumer-flip + make-public → isothrum.com editor. NOT blocked (proceeding): launcher re-point to live 1.0.0 (worker ad0b82e, +version fix 1.5.0, standalone-build verify) + isoloom/isothrum/launcher make-public + cyberctf-web/cyber-courses-web deploy. **HOLDING the final squash/make-public/deploy until npm authed so the release lands complete+consistent** (crates are already live — the core is out). 

**isoloom 1.0.0 LIVE on crates.io (2026-10-18) ✅** — all 13 crates (isoloom, -ansible, -catalogue, -compose, -import, -kubernetes, -manifest, -plan, -render, -spec, -targets, -terraform, -vagrant) verified at 1.0.0 (crates.io needs a User-Agent header to query). OIDC publish run 39085812705 success. (isoloom-external/-coverage are NOT crates — coverage removed Phase 1a.) **isothrum publish BLOCKER (coupling the audit warned of):** isothrum's `xtask` git-deps isoloom's `xtask-quality` (git rev, isoloom repo not public yet) → poisons the workspace relock; the 4 publishable crates (runner/compiler/wasm/cli) do NOT use xtask-quality (only dev xtask does), isoloom libs are registry `=1.0.0`. FIX: revert the premature isothrum-side xtask-quality dedup → isothrum keeps its OWN local quality tooling (self-contained public repo, no cross-repo git dep) → relock against registry isoloom 1.0.0 → publish. (isoloom's own xtask-quality crate stays, publish=false.)

**LAUNCHER VERSION CORRECTION (2026-10-18):** the 1.0.0-bump worker bumped the launcher 1.4.1→1.0.0 = a DOWNGRADE (the launcher is a separate product already at 1.4.x that CONSUMES the 1.0.0 engines). FIX: launcher → **1.5.0** (minor forward; applied at the launcher re-point step F + CHANGELOG dated) NOT 1.0.0. The 1.0.0 release = isoloom/isothrum/isothrum-ui/isothrum-desktop (new public packages — correct). Launcher is adjustable (desktop app, not a registry publish) — maintainer may prefer 2.0.0 to signal the rearchitecture. Flagged.

**RELEASE AUTHORIZED (maintainer, 2026-10-17) — execute once 2c-A lands.** Decisions: (1) **Run the full release sequence after 2c-A, with a MANDATORY final go/no-go confirm from the coordinator right before the irreversible squash+publish+make-public** (do all prep first, then stop for the one confirm). (2) **Make-public = isoloom + isothrum + launcher + isothrum-ui + isothrum-desktop** (Apache-2.0); websites + cyberbackend + cyber-auth stay PRIVATE. (3) **Websites: deploy** cyberctf-web + cyber-courses-web docs to prod + remove isoloom.com banner + repoint docs-sync + isothrum.com editor-on-npm-WASM + **post the 1.0 announcement**, all with the release. (4) **Node: widen `engines` to allow Node 25** (no install; code passes on 25) across launcher/isothrum-ui/isothrum-desktop/web repos.
**AUTHORIZED RELEASE SEQUENCE (after 2c-A lands, before the final-confirm gate — all PREP):** widen Node engines → finish lab→environment rename (public re-exports + desktop IPC) + quality-tooling dedup → 1.0.0 bump + changelog date + isothrum README + launcher CHANGELOG stub → isothrum-ui GitHub repo + confirm npm publish + flip consumers file:→^0.1.0 → desktop cutover (remove apps/desktop from isothrum, create+push GH isothrum-desktop, apply editor flip; ith-D1 superset migration = 1.0.x) → re-point launcher to published-1.0.0 deps (builds standalone, no dev-link) → CI re-enable (ci.yml lifecycle→runner + stale usage strings) → clean pve-lab QA token + host cargo caches → run full gates on all repos (now Node-25-OK). **THEN THE FINAL CONFIRM** → squash ALL repos → publish 1.0.0 (crates.io + npm) → fresh ESXi run vs published launcher → make-public the 5 repos → push websites + banner removal + docs-sync + isothrum.com + announcement. Non-AWS clouds as-is (1.1). OWED USER (external, can happen anytime): courses:sync scope in Cyber Auth; retag publish-lab-action; NPM_TOKEN for courses-web.

**ESXi FRESH RE-RUN → DEFERRED TO RELEASE (2026-10-17, coordinator call).** Booted esxi-lab (ESXi 8.0.3 up, key-reachable) + set up an isolated launcher worktree, but running the ESXi test NOW builds against the shared dev-link worktrees (s2-isoloom/s2-isothrum) that the 2c-A worker is actively editing → fragile. The runner-only SERVER path (ServerRun) is already re-validated via **Proxmox PASS** on 00425c6 (ESXi uses the same path + vmware_esxi provider + ovftool), and ESXi was proven 3× pre-refactor. So the fresh ESXi run is best done at the RELEASE phase against the CLEAN published-1.0.0 launcher (no dev-link, no contention — the shippable artifact). esxi-lab stopped; scratch worktree removed. **RELEASE-PHASE QA TASK:** fresh ESXi password-run (CYBERCTF_TEST_ESXI_HOST=172.16.1.20, datastore1, 'VM Network', root + password for ovftool; ed25519 agent for net-ssh) via `a_vm_lab_runs_on_esxi_through_the_runner` against the re-pointed published launcher.

**RE-VALIDATION TIERS + ESCALATIONS (2026-10-17).** Tiers `isoloom@3541e0c1` (proxmox/mac, vagrant/libvirt/friday, vagrant/hyperv/karen = pass, "runner-only launcher 00425c6"). **Escalation 1 (expected, covered):** committed launcher `00425c6` builds ONLY via the local dev-link ([patch] isothrum 34244d6 + isoloom 2a49995a) — its Cargo.toml pins published isothrum 0.11.1 but `runtime/isothrum/cloud.rs` uses `isothrum_runner::Running` (renamed API, unpublished). So the RELEASE sequence MUST publish isothrum 1.0.0 (renamed API) BEFORE re-pointing the launcher to published deps (already in FINAL). **Escalation 2 (minor cleanup, pre-ship):** pve-lab (OFF) retains QA token `root@pam!cyberctf-qa` + runner ed25519 pubkey in /root/.ssh/authorized_keys; friday/karen retain isoloom cargo-git cache — clean before v1 ship.

**REMOTE RE-VALIDATION COMPLETE + STAGE 4 BADGE LANDED (2026-10-17).** Runner-only launcher (00425c6) re-validated on all 3 key-auth remote hosts: **Proxmox/pve-lab PASS, libvirt/friday PASS, hyperv/karen PASS** (full cycles, hosts clean). So the shift is proven on 6 platforms: docker + local-VM + real-AWS cloud + Proxmox + libvirt + Hyper-V. **Stage 4 Beta badge LANDED `launcher@38bb5ac`** (local, ff from 00425c6; vitest 1288 + cargo 975 + ratchet held): data-driven off real-run EVIDENCE (validated-only=Beta [Azure/GCP/DO/Linode/OCI], real-run incl cloud-*/aws=no badge, tier3=Experimental), tooltip=evidence; new tauri-free `runtime/tiers.rs` + `run_tiers` wrapper, full chain + 6-locale i18n + tests. Pre-flight (`lab-readiness.ts` hard-block/soft-warn) + UX ErrorCode→guidance (`runtime/isothrum/errors.rs`) + bounded waits ALREADY existed (from the 2c flip) — no gold-plating. **Deferred to 1.x:** block unreachable/auth-failed saved hosts at selection (currently fails fast with a clear message). **REMAINING launcher code = 2c-A only** (attacker box via the runner: isoloom on_demand access-machine [WIP scaffolded, needs building] + isothrum hot-add + launcher rewire → delete the vm/ + exegol attacker residuals). **OWED (coordinator): ESXi fresh password re-run on the runner-only launcher** (user chose it; Mac free now — pve-lab stopped). After 2c-A, launcher is 100% runner-only incl. the attacker.

**LAUNCHER RUNNER-ONLY SHIFT COMPLETE + PROVEN (2026-10-17) — the milestone.** `cyber-ctf-launcher@00425c6` (LOCAL main, −7251 LOC across 88 files): the launcher has NO path that runs its own lab runtime — docker/vm/server/cloud/cloud-services ALL go through the Isothrum runner. Deleted the terraform driver, own docker/vm/lab exec, the lab-engine switch; reaper runner-backed; self-test connection-only; dead_code=deny restored; clippy ratchet DOWN+locked; cargo 973 + vitest 1287 + next build green (direct-verified; formal pnpm gate at release — Node env TBD). No-shell-out grep clean bar sanctioned attack-VM (vm/) + attack-box (exegol, 2c-A) + read-only status/version. **Stage 3 PROOF (shift works, all green through the runner-only launcher):** docker (hello-stack) PASS; local-VM (solo-web docker-in-VM/VirtualBox) PASS; real-AWS cloud-vm (t3.micro us-east-1, acct …3936) PASS up 9/down 9, account-wide EC2 sweep EMPTY. Remaining launcher: Stage 4 (UX+pre-flight+data-driven Beta badge) + 2c-A fast-follow (Role::Access attacker via runner, then delete vm/). **Coordinator next:** remote-host re-validation of the runner-only launcher (Proxmox/pve-lab, ESXi/esxi-lab=owed fresh password re-run, libvirt/friday, hyperv/karen) — server/VM paths were already runner-backed pre-refactor so low-risk, confirmatory. Launcher stays LOCAL (re-point to published 1.0.0 + push at release).

**CERT-CI (#145) AUTHORED + PUSHED (2026-10-17, parallel).** isoloom `f0a82200` (`.github/workflows/real-{vagrant,proxmox,cloud}.yml` + `actionlint.yaml` + `docs/ci-real-hosts.md` enablement doc) + isothrum `655934d` (`real-runner.yml`). All workflow_dispatch-only, skip-not-fail on self-hosted labels/secrets, read-only contents (upload cert-record artifact + step summary; maintainer commits tiers.json/matrix.md), ephemeral SSH keys, OIDC-preferred, AWS-only cloud (mandate #12). Rewrote the stale `real-*.yml` (they called the REMOVED `isoloom run/test/down`) onto the isothrum-runner path. **CI-RE-ENABLE TASK (squash-time):** both repos' `.github/workflows/ci.yml` docker-lifecycle job STILL calls removed `isoloom run/test/down/status/exec/reset/gc` → broken vs current CLI (isoloom is convert-only now; execution = isothrum runner). CI is OFF until the squash (local xtask gates are the real gate), so not a red-origin emergency, but the squash's "re-enable CI" step MUST: fix ci.yml's lifecycle job → isothrum-runner path (pattern = the new real-*.yml) + sweep stale source doc-comments/usage strings referencing the deleted runtime (fold into the lab→environment rename/clean-code squash sweep). Also isothrum README "0.11.0"→current at the 1.0 docs pass.

**ENGINE CLEAN-CODE LANDED+PUSHED (2026-10-17, parallel).** isoloom `03bf1714` (Version ordered by numbers not chars — semver-correct panic-free Ord, compose workaround removed, proptests; output byte-identical) + isothrum `26c164d` (exhaustive compile-guarded test over all 26 ErrorCodes, replacing 5-of-N). Quality-tooling dedup ASSESSED→deferred to the squash window (not clean zero-behavior; needs cross-repo dep; plan in `scratchpad/clean-code-deferred-engines.md`). **Clean-code review thread CLOSED** except: quality-tooling dedup + lab→environment rename-finish (both at-squash), and the 2 cyber-courses-web fixes (website-release step, need NPM_TOKEN). Sole active engineering track now = the launcher runner-only refactor (2c-B→2c-C→Stage 3).

**DESKTOP-SPLIT PREP DONE (2026-10-17, parallel).** New LOCAL repo `~/code/isothrum-desktop@3f1e2f4` (Apache-2.0, no remote) — the Tauri app stands alone, builds green (cargo build + 35 tests + `cargo tauri build --no-bundle` release); isothrum `apps/desktop` left intact. ith-D2 fixed (isoloom pin 690d585e→2a49995a). **`ui/desktop-flip@a74898c` = ONLY the release-time editor `file:`→published `^0.1.0` flip** (not separate code; deferred to cutover). **ith-D1 finding (important): the desktop runner is NOT a duplicate, it's a SUPERSET** — desktop `runner/` (~2.3K LOC) does traffic capture, nsenter port-probe, VNC, console, snapshots, link suspend/resume — none exposed by isothrum-runner (whose compose driver is private). So "desktop→isothrum-runner" (ith-D1) is a real migration needing isothrum-runner to gain those first; deferred post-launcher-refactor with a plan (expose compose-ps parser; drive lifecycle via Runner/Running; keep desktop-only features from runner status/outputs; rename lab IPC vocab ith-D3/D4 same pass). **Release-time desktop cutover:** remove apps/desktop from isothrum; create+push GitHub isothrum-desktop; apply a74898c (editor + published crate deps); ith-D1 migration. Log: `scratchpad/desktop-split-prep.md`.

**CLEAN-CODE AUDIT + PATCH (2026-10-17).** Read-only audit → `scratchpad/clean-code-audit.md` (ranked, tagged safe-now/after-refactor/at-squash; positives: no authz gaps, launcher docker-in-VM NOT duplicated). Patcher applied safe-now in quiescent repos only: **cyber-course-sync `lab.json`→`.ctf/metadata.json` FIXED+landed `cyber-course-sync@be0548b`** (local; author guidance pointed at a file publish-lab-action never reads). **cyber-auth `courses:sync` scope ALREADY on main `392d2ea`** (audit read a stale branch) — so the owed item is ONLY the external Cyber-Auth IdP registration, not a code gap. **cyber-courses-web** cookie rename (`cyberctf_*`→`cybercourses_*`) + img remote-allowlist restore: authored + backed up in scratchpad, NOT committed — gate needs the private `@cyber-courses/*` registry token (NPM_TOKEN) absent here; **fold into the website-release step** (token available then) or land sooner if the maintainer drops NPM_TOKEN. **DEFERRED clean-code (after-refactor/at-squash):** isoloom-manifest Version lexicographic Ord (correctness: 2.9.0>2.14.0), isothrum ErrorCode↔Error 25-variant sync test (only 5 tested), isothrum-desktop isoloom-rev pin + desktop-second-runner→isothrum-runner, launcher LabEngine vestigial toggle (removed by the refactor anyway), duplicated quality tooling (web scripts/quality/* + rust xtask/quality/*), and the rest of the lab→environment rename (public API re-exports + desktop IPC still carry Lab/LabName — one isoloom-first squash-time pass). Patch log: `scratchpad/clean-code-patches.md`.

**2c RE-SEQUENCED (2026-10-17, coordinator call, speed-aligned).** Realizing the attacker-via-isothrum feature (isoloom-generates launch-time learner-image machine + new isothrum hot-add op w/ compose+vagrant drivers + hundreds of snapshot regens) is DAYS of engine work; ordering it first blocked the whole runner-only shift + the Stage-3 proof. New order: (1) 2c-B+2c-C now → lab execution 100% runner-only (delete terraform/proxmox/cloud_services-exec/docker-exec/ssh+server-runner-halves/switch/~294 tests), **keep `vm/` SOLELY for the learner's attack VM = the one explicitly-flagged temporary residual** (no-shell-out grep shows only that, documented, tracked for removal in 2c-A); (2) Stage 3 (flag-portal docker + local-VM solo-web + real-AWS cloud-vm zero-leftover) = the "shift works" proof, in hours; (3) Stage 4 (UX+pre-flight+Beta badge); (4) **2c-A as fast-follow** = attacker machine via isothrum (isoloom-generates + hot-add, WIP `on_demand` field started in isoloom worktree) → then delete `vm/` → zero residual. This temporarily reverses "attacker must not be launcher-Vagrant" (interim only; end state unchanged). CLEAN-CODE AUDIT (read-only, a854ad) running → report scratchpad/clean-code-audit.md; on completion a PATCH worker applies the safe-now findings in quiescent repos (not refactor-active files).

**LAUNCHER DECISIONS (maintainer 2026-10-17):** (1) **Attack VM** → added THROUGH isothrum as a machine placed on a lab network; the launcher UI lets the user choose which network when there are multiple, auto-joins when there's one; no launcher Vagrant. Likely a new isothrum "add a machine to an environment/network" capability. (2) **Server self-test** → (coordinator call, user deferred) reframe as a runner-run probe (spin+teardown a trivial 1-machine lab on the host via the runner); fallback = sanctioned admin-only exception if heavy. (2/3/5 mechanical: shell+LAN-relay read host from runner status/outputs; server var-building relocated out of the deleted exec module; adopt-path reads minimal runner state.)

**LAB-CORPUS PROOF SIZED + UNDERWAY (2026-10-17) — the long pole shrank.** Track A DONE (`isoloom-corpus@a7c7b80`, local): 28 flag scripts repointed env→file, all Docker-scope todos validate (the other 49 of 84 are VM/AD/k8s, out of Docker scope → their platforms). Track B (`ff91a2f`): sealed-jail mechanics green (seal check passes, `/run/lab` delivery, clean teardown); **content defect** — 26 google chroot jails place the flag via `setup:` which runs in a separate init container (shares network, not the service rootfs) so the write is discarded; fix = deliver/read the flag in the challenge image entrypoint (xben pattern); 1 fixed, 25 being ground now. Track C **re-sized: only 74 Docker labs need upstream-source restore, NOT ~409** (288 run as-is); pins = `CyberCTF/<lab>@<sha>` (maintainer org forks); pipeline proven on 6 families; **~1.5–2.5h, batchable, no auth friction**; restores kept ephemeral (corpus stays lean). Worker resumed to grind the 25 jail fixes + 74 restores. Corpus is LOCAL only (no remote). **Engine note to document:** `setup:` affects volumes/files/network, not the service image rootfs (runs in a separate init container).

**MAINTAINER ITEMS DONE (2026-10-17, worker).** (1) AMOUNT fix already landed `cyberbackend@d2f1ff8` (the "proposal only" line was stale) — verified, 86 Neo4j e2e green. (2) Course-sync authz (Finding 7) already landed `cyberbackend@969ad6f` (`courses:sync` scope → `course-sync` role; `COURSE_SYNC_CLIENT_IDS` allow-list) — verified. (3) publish-lab-action `isothrum_version` wired: `lab-template@39f2e19` + `publish-lab-action@8826bd0` (local). (4) C3/C4 base-image pinning `isoloom@0f6f668a` (Proxmox/docker-vm/ESXi dated URLs + checksums; ESXi provider has no checksum slot → URL-only pin, real ESXi path uses vagrant box checksums; gates verify 0). **isoloom main staged (NOT yet pushed — bundling with the rename landing):** `a9c748be` env-key rename + `0f6f668a` C3/C4. **OWED TO MAINTAINER (external, not code):** register the `courses:sync` scope in Cyber Auth + grant to content-repo clients; retag `publish-lab-action` so lab CI `@v1` resolves; a periodic image URL+digest refresh (follow-up, not built).

**MAINTAINER APPROVALS (2026-10-17) — the finish-line mandate.** Do all of robustness 1–7 (runtime-unification + lab-purge + UX + pre-flight + CI matrix + real-host cert CI). (8) **SQUASH EVERYTHING** (all repos' history) at the end. (9) **Desktop split GO** (isothrum-desktop from ui/desktop-flip@a74898c), after the isothrum rename lands. (10) **Website release work APPROVED** (push cyberctf-web `cc62cb0` + cyber-courses-web `48f1a93`, remove isoloom.com banner, repoint docs-sync, isothrum.com editor-on-npm-WASM, release announcement) — sequenced AT release, not before (they describe 1.0). (11) **1.0.0 bump** when everything done + ready to squash. (12) **Non-AWS clouds (Azure/GCP/DO/Linode/OCI) ship AS-IS for v1** (Tier-2 generate+validate); real runs are 1.1. (13) **Do the maintainer-owed code items** (AMOUNT fix, course-sync authorization, publish-lab-action isothrum_version wiring, C3/C4 image pinning) — local only. (14) **Capability matrix is enough** — do NOT rebuild the literal per-format coverage metric. **Make-public scope:** isoloom/isothrum/launcher/isothrum-ui (Apache-2.0) public; websites + cyberbackend + cyber-auth stay PRIVATE. **Final-sequence order:** finish 1–7 + lab proof + ESXi + maintainer items → 1.0.0 bump + changelog → isothrum-ui GitHub+npm publish + flip consumers → desktop split → **final "ready to squash?" confirm** → squash everything → publish 1.0.0 → make-public → website pushes + banner + docs-sync + announcement.

**For the maintainer when back:**
1. **Push hotfix?** The cyber-courses-web `hotfix/content-fetch` (`099adc6`) fixes a production security flaw in the lesson fetch. All 9 published lessons already store a commit ref, so none breaks. **Push it alone?**
2. **Two approvals:** the AMOUNT decimal fix (about 1 player in 10 can't solve invoice-portal-api), and a separate `courses:sync` scope in Cyber Auth.
3. **Production admin action:** unpublish or delete the placeholder DRAFT lab `invoice-portal-sqli` until the real one exists.
4. **Azure cost:** machines on 3 to 8 networks now get `Standard_D8as_v5` (B1s can't take 3 NICs, and B-series v1 retires). The cheaper v6 sizes need Gen2 images.
5. **Two free 10.0.0.x addresses** for the bridged LAN lab VMs.

## 0. Three products

| Product | Role | Contains |
|---|---|---|
| **Isoloom** (this repo) | Converter: one spec to every IaC **and configuration management** tool, and back | Spec, catalogue, plan, backends, import, `verify`, and the **run manifest** each target outputs. Apache-2.0. No runtime. |
| **Isothrum runner** (crate and CLI in `isoloom/isothrum`) | Runner: executes what Isoloom generates | One engine with: lifecycle (up, down, park, resume, re-provision); state and locks; a driver per target; secrets delivery; checks; readiness and recovery; reaper and budgets; snapshots, capture and consoles. Shared by both apps below. |
| **Isothrum desktop app** | UI wrapper with **dynamic specs** | The same logic as the launcher, but the spec is drawn or edited live in Isothrum's lab editor (a drawing compiles to a spec) instead of coming from a fixed catalogue. It uses Isoloom to convert and the Isothrum runner to run. |
| **Cyber CTF launcher** | UI wrapper with **fixed specs** | Curated CTF lab catalogue, accounts, attack box and player UX. It uses Isoloom to read and convert labs, and the Isothrum runner to run them. |

**Repositories:**

| Repository | Licence | Contains | Used by |
|---|---|---|---|
| `isoloom/isoloom` | Apache-2.0 | Converter crates and CLI | everything below |
| `isoloom/isothrum` | Apache-2.0 | Libraries only: the runner, the lab-model compiler (drawing to spec, today's `isothrum-core`) and its WASM build | desktop app, website (WASM), launcher (runner) |
| `isoloom/isothrum-desktop` (new) | Apache-2.0 | The Tauri app: brings Isoloom, the runner, the compiler and the UI together, with dynamic specs | users |
| `isoloom/isothrum-ui` (new, optional) | Apache-2.0 | Shared front-end components: editor canvas, lab view, machines, consoles | desktop app, website, possibly the launcher |
| `isoloom/isothrum-website` | Proprietary, private | isothrum.com: marketing site and blog, the in-browser lab editor (`desktop-editor/`, React Flow), accounts (Better Auth), the project store (`db/`), and the MCP server (`/api/mcp`). Uses the WASM engine (committed today under `public/wasm/`) and later `isothrum-ui` | visitors, Isothrum users |
| `isoloom/isoloom-website` | Proprietary, private | www.isoloom.com: marketing site, blog and the docs, synced from the Isoloom repository by `scripts/sync-isoloom.mjs` | visitors, Isoloom users |
| `Cyber-Courses/Cyber-CTF-Launcher` | Apache-2.0 | CTF UI with fixed specs; uses Isoloom and the runner | players |

**Packages, not pinned git commits:**
- **Rust:** Isoloom and Isothrum crates are published to crates.io.
- **JavaScript:** the UI and WASM packages go to npm.
- **Why:** consumers depend on versions, so the Isoloom history squash can't break them, and each one upgrades on its own schedule.

**Contracts:**
- **Isoloom to Isothrum:** the generated files plus a versioned run manifest, holding:
  - native commands and their order;
  - readiness criteria;
  - outputs: addresses, host ports, URLs;
  - how to run checks;
  - secret slots: names and destinations, never values.
- **Isothrum runner to UIs:** a Rust API plus a JSON event stream, used by both apps.
- **The two apps share most of their logic:** lab view, machines, consoles, start/stop, logs, checks. A shared app core (Rust plus UI components) can be extracted once both run on the runner, so a feature is built once.

**Runner code comes from three runtimes, merged into one in Isothrum:**
- **The launcher's** (about 16k lines, the most mature) is the core.
- **Isoloom's run part** moves in rather than being deleted: secrets delivery including Windows, host readiness, the registry and WinRM retries.
- **Isothrum desktop's** switches onto the result.

## 1. What Isoloom 1.0 is

**One spec, converted to every other infrastructure-as-code and configuration-management tool.**

Two layers are converted:
- **Infrastructure (IaC):** networks, VMs, containers, clouds.
- **Configuration (CM):** what is on each machine: packages, services, files, users, config, commands.

The spec describes both once, and the user picks a tool per layer: Terraform + Ansible, Vagrant + Puppet, Compose + Dockerfile, Proxmox + cloud-init.

- **Isoloom is a converter.** It reads one `isoloom.yml` and writes the native files of each tool: Compose, Kubernetes manifests, Helm, Kustomize, Vagrantfiles, Terraform/OpenTofu modules for Proxmox, ESXi and the clouds, cloud-init, and Ansible inventories and playbooks.
- **It also does the reverse:** it imports those formats into a spec (`isoloom import`).
- **It doesn't run anything.** Users, CI and the Cyber CTF launcher run the native tools on the generated files.
- **One declaration per machine.** A machine says what it is: an OS and the services it runs, from a catalogue. It never says how a platform runs it, so there are no `docker:` or `vm:` blocks. Each target decides how to deliver it: an official container image or a built image on container targets; a fresh VM plus an Ansible role on VM targets.
- **Honest proof.** Every generated file passes its format's official validator. Each target sits in a tier that says how far it has been proven.

## 1a. The launcher's promise sets 1.0's priorities

Isoloom exists for the Cyber CTF launcher. Its three modes must work in 1.0, on macOS, Windows and Linux hosts:

| Launcher mode | What runs | Isoloom targets (Tier 1 for 1.0) |
|---|---|---|
| **Local** (`Container`, `LocalVm`) | Docker on the player's machine, or VMs through Vagrant on VirtualBox, Hyper-V, VMware or QEMU/libvirt. Containers can also run inside a VM | `docker`, `vagrant` (every local provider), `docker-vm` |
| **Server** | The player's own Proxmox or ESXi server, through Vagrant (`vagrant-proxmox`, `vagrant-vmware-esxi`, configured through the launcher's `connection_env` contract) or Terraform | `proxmox` (Terraform), the Vagrant Proxmox and ESXi providers |
| **Cloud** | The player's AWS, Azure, GCP, DigitalOcean, Linode or Oracle Cloud account, through Terraform with each provider's CLI credentials | `cloud-vm`, `cloud-docker`, `cloud-services` |

**Rules that follow:**
- **Terraform is core to 1.0** for server and cloud. Only the Terraform provider plugin is dropped.
- **Every phase does the launcher's targets first.** Kubernetes, Helm, Kustomize, hybrid, hosted, external and the Terraform ESXi target follow. They stay ("it's the deal") but aren't Tier 1 for 1.0.
- **Contracts the launcher relies on are kept and tested:** the Vagrant providers reading their connection from the environment; the cloud-services module layout, state and inputs; the check runners.
- **Real proof needs real infrastructure.** Local is proven on this Mac now. Server mode needs a Proxmox or ESXi host, and cloud mode needs budget-capped accounts. Without them, server and cloud ship as generated and validated, not as proven.

## 2. Decisions taken (2026-10-15)

| Topic | Decision |
|---|---|
| Scope | Converter only. The runtime goes: `run`/`test`/`down`/`status`/`connect`/`exec`/`capture`/`reset`/`provision`, the registry, JSON lifecycle events, run-time secret delivery and Isoloom-executed checks. |
| Coverage | `isoloom-coverage` (13k lines, 30 format tables, refresh scripts, drift job) is replaced. What stays: a spec-field × target capability matrix generated from each backend's declared capabilities, and conformance against official validators. |
| Syntax | One form: `os` plus `service`(s) from the catalogue, or a generic custom `app` (own image, or a build recipe). A small per-target `overrides:` handles rare platform specifics. No `docker:`/`vm:` blocks. |
| Catalogue | Data-driven entries: a container form (official image, or a build from the OS base image), a VM form (official Galaxy content only, else our own role), and a parity contract. |
| Breadth | Every current target stays ("it's the deal"), placed in tiers. The launcher's targets (local, server, cloud) come first in every phase and are Tier 1 for 1.0; see 1a. |
| Tiers | **Tier 1:** proven by real runs from every host platform the target exists on (macOS, Windows, Linux). **Tier 2:** generated and validated by the official validators. **Tier 3:** experimental. The tier is shown in the matrix and in the CLI. |
| Host platforms | Every running platform is supported: macOS, Windows and Linux. The CLI ships for all three, and generated output must run from all three hosts: no `sh`-only `local-exec`, no host path separators or line endings baked in, no tool assumed that a host lacks. A target that only exists on some hosts (Hyper-V on Windows, libvirt on Linux, Parallels and UTM on macOS) is proven on those. |
| CM tools in 1.0 | Ansible, cloud-init, shell/PowerShell and Dockerfile. Puppet, Chef, Salt and PowerShell DSC follow in 1.x. |
| IaC tools in 1.0 | The current families: Compose, Kubernetes/Helm/Kustomize, Vagrant (every provider), Terraform/OpenTofu for Proxmox, ESXi and the six clouds. Pulumi and Podman: not yet. |
| Secrets | A conversion layer per tool: the spec names secrets and where they come from, and each output gets that tool's native mechanism. Values never land in a generated file. On clouds that means the cloud's own store: AWS Secrets Manager or SSM, Azure Key Vault, Google Secret Manager. Where a cloud's store doesn't fit (DigitalOcean, Linode, OCI), a documented SSH guest-delivery fallback is used. |
| Terraform provider | Only the provider plugin is dropped for 1.0 (recommended; the maintainer had no preference). Terraform output (Proxmox, the clouds, cloud-docker, cloud-services) is core to 1.0. In a converter, Terraform users point a `module` at the generated folder or use the published registry modules. Revisit later as an Isothrum provider that runs labs. |
| Licences (everything open source, least constraints) | **Isoloom, Isothrum (runner, desktop app, editor) and the Cyber CTF launcher:** Apache-2.0. It is permissive, with a patent grant and patent retaliation, explicit contribution terms, and no trademark rights granted, so forks can't use the Isoloom or Isothrum names. No CLA, only a DCO sign-off. Register the names as trademarks if branding matters. Anyone may build anything on them, closed products included. Official Ansible collections (GPL-3.0) are referenced, never embedded in a binary. The Isothrum website (accounts, project store, MCP) stays private and proprietary. The CTF lab content and courses are a separate decision. |
| Compatibility | **None.** The project isn't even in beta, so 1.0 supports only the new format: no aliases, no deprecations, no `isoloom migrate`, no tests on old specs. Old specs fail validation with an error naming the unknown field. The spec stays at `version: 1`, the first stable format. Examples are rewritten by hand. The CTF lab files are rewritten by hand at the end, before the launcher's release. |
| Workflow | Everything lands directly on each repository's local `main`, fast; parallel workers use scratch worktrees only to avoid clashing, and fast-forward `main` themselves. No PRs, no lingering branches, no releases. The three repositories sit side by side with **path dependencies** (no git revs) until the 1.0 publish. One script runs every repository's checks. At the first satisfying version: squash, re-enable CI, publish. |
| Docs | Written at the end, inside the repo. The website is rewritten from them. |
| Launcher | **Decided:** integrate it from the walking skeleton (Phase 2) on, instead of after 1.0. It is the reason 1.0 exists, and only it can validate the run manifest. Launcher, runner and Isoloom ship 1.0 together. |
| GitHub | Done: PRs and issues closed, releases and tags deleted, workflows disabled. |

## 3. Target architecture

**Configuration model.** A small, typed, tool-neutral resource model sits in the plan:
- `repository`, `package`;
- `file`, `template`;
- `user`, `group`;
- `service`;
- `command`, with an already-done guard;
- `firewall rule`.

Catalogue entries and machine settings are written once in it. Configuration backends render native code from it:
- Ansible tasks and roles;
- cloud-init;
- shell and PowerShell;
- Dockerfile;
- Puppet manifests, Chef recipes, Salt states, PowerShell DSC.

Parity between the container and VM forms holds by construction, and the parity contract still checks it.

**Pairing rules (IaC × CM):**
- Vagrant calls Ansible, Puppet, Chef or Salt as native provisioners.
- Terraform hands off to the chosen CM tool.
- Container targets bake configuration into the image.
- cloud-init configures at first boot.

```
isoloom.yml ──► isoloom-spec ──► isoloom-plan ──► backends ──► native files
 (one form)     (model,          (resolve os +     (compose,      (Compose, K8s, Helm,
                 validate,        service through   kubernetes,    Kustomize, Vagrant,
                 validate)        the catalogue,    vagrant,       Terraform, cloud-init,
                                  the environment)  terraform,     Ansible)
                                                    ansible)
                isoloom-catalogue                   isoloom-render (builders: HCL, Ruby, YAML,
                (entries, roles,                     shell, PowerShell, Dockerfile)
                 contracts)
                isoloom-import (native files ──► spec)
                isoloom-verify (official validators on the generated files)
                isoloom-targets (registry, capabilities, the matrix, tiers)
                isoloom (CLI: generate, validate, import, verify, matrix)
```

- **Lowering, not rewriting:** the new spec is a front end. It resolves `os` + `service`/`app` and lowers each machine into today's plan IR (`Environment`), which already carries the container and VM forms. The backends keep working, and their 800+ tests keep guarding them. Internals are reshaped only where a feature needs it.
- **Removed:** `isoloom-runtime`, `isoloom-coverage`, and the runtime parts of the CLI.
- **New:** `isoloom-catalogue` and `isoloom-verify`.
- **Changed:** `isoloom-external` becomes the Ansible output (existing hosts are an inventory plus playbooks).

## 3b. Engineering standard (every repository, every phase)

**All clean code principles are respected in all three codebases** (maintainer, 2026-10-15). Clean code is built in as we go, not left for a later pass.
- **Enforced by a tool** wherever a tool can check it.
- **Otherwise checked by the review checklist below** at the end of every phase.
- **The target is zero violations in every repository by 1.0,** not just "no worse".

### Design rules
- **Functional core, imperative shell.** Parsing, validation, planning, rendering and manifest building are pure functions over data, with no I/O, no clock and no environment. All I/O (processes, files, network, Tauri) sits in a thin shell at the edge. The pure core is unit-testable without mocks.
- **Ports and adapters** for everything that touches the outside world:
  - **Runner:** drivers (Docker, Vagrant, Terraform, Proxmox, clouds) sit behind a `Driver` trait, and the process runner and the state store are traits too. Tests use fakes, never a real tool.
  - **Launcher:** Tauri commands are thin adapters that call plain Rust services. Domain logic never imports `tauri`.
- **Dependency direction is one-way, and a test checks it.**
  - **Isoloom:** spec → plan → render → backends → targets → cli. No backend depends on another backend.
  - **Isothrum:** compiler and runner, with no UI.
  - **Apps:** depend on libraries, never the reverse.

  Cyclic or upward dependencies fail the build.
- **Parse, don't validate.** Raw input is converted once into types that can't hold invalid states:
  - newtypes for names, ports, CIDRs, versions and IDs;
  - enums instead of booleans and strings;
  - no `Option` soup.
- **Errors are typed and actionable.**
  - One `thiserror` enum per library crate.
  - Messages name the field path, the reason and the fix.
  - `anyhow` only in binaries.
  - No `unwrap`, `expect` or `panic` outside tests.
- **No hidden state:** no globals, no `set_var`, no `unsafe`. Time, randomness, the environment and the filesystem are passed in.
- **Determinism:** ordered maps in all output, and render-twice tests.
- **One way to do each thing.** A helper exists once. A second copy fails review, and a duplicate-code check flags it.

### Size and shape limits (lint-enforced)
- **Rust:**
  - functions ≤ 60 lines, ≤ 5 parameters, cognitive complexity ≤ 15;
  - files ≤ 400 lines of code;
  - clippy `pedantic` at deny, with a short commented allow-list;
  - `missing_docs` on libraries.
- **TypeScript:**
  - `strict`, with no `any`;
  - typescript-eslint `strict-type-checked`;
  - components ≤ 200 lines, functions ≤ 60;
  - no default exports outside framework entry points;
  - Tauri types generated from Rust (`ts_bindings`), never hand-copied.
- **Naming:** one vocabulary across all three repositories (lab, machine, network, target, form, manifest, run, driver). Comments say *why*, never *what*.

### Tests
- **The pyramid:**
  - unit tests for the pure core;
  - `insta` snapshots for every renderer;
  - property tests for parsers, address planning and quoting (HCL, Ruby, shell, PowerShell injection safety);
  - contract tests: Isoloom and Isothrum test against the same run-manifest fixtures;
  - fake-tool tests for drivers;
  - real runs only for Tier 1.
- **Every bug fix starts with a failing test.**
- **Fast and deterministic:** no sleeps, no network in unit tests. The unit suite of each repository runs in under a minute.

### Principles and how each is checked

| Principle | Checked by |
|---|---|
| Meaningful, consistent names; one vocabulary | review checklist, `typos` |
| Small functions that do one thing; one level of abstraction | clippy `too_many_lines` 60, cognitive complexity 15, eslint `max-lines-per-function` |
| Few arguments; no flag arguments | clippy `too_many_arguments` 5, `fn_params_excessive_bools`; review (enums over booleans) |
| Single responsibility, small modules | file ≤ 400 LOC (TS components ≤ 200), the module layout in 3b, review |
| Open/closed, Liskov, interface segregation, dependency inversion | ports-and-adapters traits, small traits, review |
| DRY | duplicate-code check (`jscpd` across Rust and TS), review |
| KISS and YAGNI | review: no speculative abstractions, no unused generality; `cargo machete` and `knip` for unused deps and exports |
| Command-query separation; no hidden side effects | functional core, pure functions, review |
| Law of Demeter | review: no long call chains through other objects' internals |
| Errors as types, not codes or strings; no null | typed errors, `Option`/`Result`, no `unwrap`/`expect`/`panic`, TS `strictNullChecks` |
| Clean boundaries with third-party code | adapters around tools, processes, Tauri, HTTP and SDKs |
| Clean tests (F.I.R.S.T.: fast, independent, repeatable, self-validating, timely); one concept per test | unit suite under a minute, no sleeps or network, review |
| Comments explain *why*; no commented-out code, no noise | review, the eslint and clippy rules for commented-out code |
| Formatting | rustfmt, prettier |
| No dead code | `dead_code` at deny, `cargo machete`, `knip` |

### Ratchet to zero
- Each repository has `quality-baseline.json`: the current count of violations per rule, such as oversized files, long functions, `allow`s, `any`s and duplicates.
- A check fails if any count goes **up**, and every phase drives the counts **down**. The ratchet replaces launcher issue #150.
- **All counts reach zero before the 1.0 release:**
  - Isoloom by the end of Phase 1;
  - Isothrum when the runner lands (Phase 2–3);
  - the launcher's non-runtime code in Phase 0.6;
  - the launcher's runtime as it moves into the runner.
- The only allowed exceptions are local `#[allow]` or `eslint-disable` lines with a written reason, listed in the baseline and reviewed.

### Process
- **Every change:** format, lint, test, the ratchet and the dependency-direction check, all through `check-all`.
- **Every phase ends with a review pass** over its own diff: a code review for bugs, a simplify pass for duplication and dead code, and the principles table as a checklist. Findings are fixed before the next phase starts.
- **Docs stay alignable.** Every phase appends what it changed to `plan/docs-facts.md` (one line per user-visible fact: field, command, target, behaviour, limit). Generated docs (spec reference, targets matrix, catalogue, CLI help, manifest schema) stay current through their stale-file tests. No hand-written docs are written before Phase 7.
- **Dev tasks are `cargo xtask` commands, not shell scripts,** so they run the same on macOS, Windows and Linux. `scripts/*.sh` move into `xtask` in Phase 1.

### Repository layouts
- **Isoloom** (applied in Phase 1, after the snapshots land, so moves are proven output-neutral):

  ```
  crates/core/      isoloom-spec, isoloom-catalogue, isoloom-plan, isoloom-render
  crates/backends/  isoloom-compose, isoloom-kubernetes, isoloom-vagrant, isoloom-terraform,
                    isoloom-ansible (today's isoloom-external), and the CM backends as they arrive
  crates/tools/     isoloom-import, isoloom-verify
  crates/targets/   isoloom-targets (registry, capabilities, tiers)
  crates/cli/       isoloom
  xtask/            check-all, verify, dev-link, snapshots
  catalogue/        data: <service>/{entry.yml, contract.yml, roles/}
  examples/         specs only, no committed outputs
  schemas/          generated JSON Schemas: spec, run manifest
  tools/verify/     the pinned validator image
  .github/          community files (CONTRIBUTING, SECURITY, CODE_OF_CONDUCT) and the disabled workflows
  ```

  - `rfcs/` and the hand-written `docs/` are removed now and rewritten at the end.
  - `install.sh` moves to `packaging/`.
  - `rust-toolchain.toml` pins the toolchain.
- **Isothrum** (applied in Phase 2, when the runner arrives):

  ```
  crates/isothrum-compiler/  today's isothrum-core (drawing → spec)
  crates/isothrum-wasm/
  crates/isothrum-runner/    manifest executor, Driver trait, drivers as modules, state store, events
  crates/isothrum-cli/
  apps/desktop/              until it moves to isothrum-desktop (1.x)
  xtask/
  ```

  The WASM npm package name stays the same, so the private website keeps building.
- **Launcher:**
  - **Now:** reorganize everything except `runtime/`.
    - Group the loose `src-tauri/src/*.rs` modules into domains: `app/` (window, menu, quit, logging, config), `labs/`, `account/`, `terminal/`, `platform/` (fs, atomic files, MSIX data, exec), `cloud/`, and `commands/` (thin Tauri adapters only).
    - Use one module convention (`foo.rs` plus `foo/`, never `mod.rs`).
    - Group the packaging assets (`store/`, MSIX) under `packaging/`, and the attack-box image under `images/attack-box/`.
  - **Phase 2–3:** `runtime/` moves into `isothrum-runner`, and the launcher keeps only CTF-specific code.

## 3d. Rust best practices (Isoloom, Isothrum, the launcher's `src-tauri`)

These go beyond lints. Each one names the tool or review that enforces it.

**Workspace hygiene**
- **Workspace settings:** edition 2024 and resolver 3. Every crate inherits `[workspace.package]`, `[workspace.dependencies]` (one version per dependency, declared once) and `[workspace.lints]` (`lints.workspace = true`). Checked by an `xtask` test over `cargo metadata`.
- **Pinned and declared versions:** `rust-toolchain.toml` pins the toolchain, `rust-version` declares the MSRV, and `cargo msrv verify` (or `cargo hack --rust-version check`) proves it.
- **Features:** additive only, never mutually exclusive. `cargo hack check --feature-powerset` proves every combination builds.
- **Supply chain:** `Cargo.lock` is committed. `cargo deny check` covers advisories, licences (an allow-list compatible with Apache-2.0), duplicate versions (`bans`) and sources (crates.io only, no git dependencies at release). `cargo machete` catches unused dependencies.

**API design** (the Rust API Guidelines checklist, reviewed per public item)
- **Common traits:** public types implement the traits a user expects (`Debug` always; `Clone`, `PartialEq`/`Eq`, `Hash`, `Default`, `Display` where they make sense; `Send`/`Sync` asserted in tests for types that cross threads).
- **Conversions:** through `From`/`TryFrom`/`AsRef`/`FromStr`, not ad-hoc `to_x()`/`from_x()` functions. Constructors are `new` or a builder for more than about three optional fields.
- **Types carry meaning:** newtypes for every identifier and quantity; enums instead of `bool` and stringly modes; `#[non_exhaustive]` on public enums and structs that may grow; sealed traits for traits not meant to be implemented outside the crate.
- **Ownership:** borrow in arguments (`&str`, `&Path`, `impl AsRef<Path>` where it helps), return owned values, and `Cow` only where it measurably saves copies. No `clone()` to satisfy the borrow checker without a reason; clippy `redundant_clone` and review catch it.
- **`#[must_use]`** on pure functions and builders (clippy `must_use_candidate`). Iterators over index loops. No `Rc<RefCell<…>>` in library APIs.
- **Visibility:** `pub(crate)` by default (`unreachable_pub`). Every `pub` item is deliberate, documented and covered by `cargo semver-checks` once the crate is published.

**Errors**
- **Library errors:** one `thiserror` enum per crate, with a `pub type Result<T> = std::result::Result<T, Error>`. Variants carry their `#[source]`, so the cause chain is never lost. No `Box<dyn Error>`, `anyhow` or `String` errors in libraries; `anyhow` is allowed only in binaries and `xtask`.
- **Messages:** lowercase, no trailing period (API guideline C-GOOD-ERR). User-facing text with the field path, reason and fix lives in one rendering layer, not scattered.
- **No panics in library code paths,** enforced by lints. A truly impossible state is an `#[expect]` with its reason, or better, a type that makes it unrepresentable.

**Concurrency and I/O** (the runner especially)
- **Sync or async, chosen once per crate and documented.** In async code: no blocking calls (`spawn_blocking` for processes and files), cancellation-safe steps, timeouts on every external call.
- **Observability:** structured `tracing` spans and fields per run, step and driver in the runner. No `println!` or `eprintln!` outside binaries. The pure core logs nothing.
- **Processes** are spawned only through the `ProcessRunner` port, with argv vectors only, never a shell string.

**Documentation**
- **Crate and item docs:** every crate has crate-level docs (what it is, where it sits, a runnable example). Every public item is documented, with `# Errors` and `# Panics` sections (clippy `missing_errors_doc` and `missing_panics_doc`).
- **Docs build clean:** doctests run and pass; `RUSTDOCFLAGS=-Dwarnings cargo doc` with `broken_intra_doc_links`, `private_intra_doc_links` and `missing_crate_level_docs` at deny.

**Testing depth**
- **`cargo nextest`** is the test runner: each test runs in its own process, with slow-test reporting and retries off, so flaky tests fail loudly.
- **Naming:** tests are named for the behaviour (`refuses_a_service_on_a_container_target_when…`). One concept per test, no `#[should_panic]`, no sleeps, and a fake clock where time matters.
- **Property tests** (`proptest`) for every parser and every quoting or escaping function. **Fuzzing** (`cargo fuzz`) for the spec parser, the manifest parser and the HCL/Ruby/shell/PowerShell quoting: each target runs in `check-all --deep`, and every crash it finds becomes a regression test.
- **Mutation testing** (`cargo mutants`) on the core crates (spec, plan, render, manifest) and the runner's executor. Every surviving mutant is either killed by a new test or listed in `mutants.toml` with its reason (an equivalent mutant). That proves the tests check behaviour, not just run code.
- **Snapshots** (`insta`) only for whole generated files. Never snapshot something a precise assertion should check.
- **`unsafe`:** the single `sys` module (runner and launcher) runs under Miri where the platform allows, and every `unsafe` block carries a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).

**Performance budgets**
- Generating every example on every target stays under a fixed time budget, checked by a test, so regressions show.
- Hot paths (spec parsing, lowering, rendering) get `criterion` benchmarks run by `cargo xtask bench`, compared against a stored baseline.

**Where it applies**
- **Isoloom:** Phase 1c (the tools are wired in by the 1c tooling unit, and every crate meets it at 1c's exit).
- **Isothrum:** from its first line (the runner skeleton follows it now).
- **The launcher's `src-tauri`:** as its code is cleaned (non-runtime now, runtime as it moves).

## 3c. The two websites

| | isoloom.com (`isoloom/isoloom-website`) | isothrum.com (`isoloom/isothrum-website`) |
|---|---|---|
| **Stack** | Next.js 16, React 19, Tailwind 4, Base UI, next-md-blog; Vercel | The same, plus React Flow (the editor), Better Auth, `mcp-handler` and a SQL project store; Vercel |
| **Size** | About 7.5k lines of TS/TSX, 43 Markdown pages (42 docs) | About 17k lines of TS/TSX, 50 Markdown pages (blog, glossary) |
| **Depends on** | Isoloom's docs, via `sync:isoloom` | Isothrum's compiler and WASM build, and the Isoloom spec the editor writes |
| **Licence** | Proprietary, private | Proprietary, private |

**Rules:**
- **A push to `main` deploys production** (Vercel): both sites are live. So they follow the same local-`main` workflow, and nothing is pushed until a release, with the maintainer's OK at that moment.
- **The engineering standard (3b) applies in full:** lint levels, the ratchet to zero, `knip` and `jscpd`, components ≤ 200 lines, typed boundaries.
- **The two sites stay in separate repositories** (maintainer, 2026-10-15). Their shared scaffolding is not merged; each repository meets 3b on its own.

**The Cyber CTF website** (`cyberctf-web`, Cyber-Courses/CyberCTF-Web, Next.js 16; labs catalogue from CyberBackend, per-player evidence submission, launcher download, docs, dashboard). Maintainer (2026-10-16): "let's make cyber ctf great as the launcher". It follows the same rules: local `main`, no PRs, no push without the maintainer's OK (a push deploys production).
- **Running now:** consolidating its local branches onto `main` (archived first), the 3b standard to zero, Vitest plus Playwright, and security tests pinning the audit's fixes.
- **Bug found (2026-10-16):** the launcher's `labs::launch_env` dropped every `CTF_FLAG_<KEY>` CyberBackend sends (it kept only `CTF_API_URL` and `CTF_LAUNCH_TOKEN`), so per-player flags never reached any lab on either engine. Being fixed test-first; flags must arrive as files, never in the environment.
- **Decided (2026-10-16): dev's merged PRs #14 (lab objectives) and #17 (lighter catalogue payloads, SEO, cyberctf.org) move onto `main`.** The single evidence form and `submitEvidence` are removed. The lab page submits per objective through `submitObjective(labId, key, answer)`, and a lab without objectives uses the implicit key `main`. Production CyberBackend already serves objectives. Once `main` carries #14 and #17, `dev` can be retired in the GitHub cleanup.
- **Running now:** an end-to-end check of the flag contract, now per objective: CyberBackend's per-player flag for each objective key → the launcher (`CTF_FLAG_<KEY>`) → Isoloom `files: [{input}]` → the runner's slot → the lab → cyberctf-web's `submitObjective` for that key → CyberBackend. Both single-`main` and multi-objective labs are covered, and the key normalisation is pinned by a test.
- **Waiting on the maintainer:** work in CyberBackend (`~/code/cyberbackend`, local main at `cc749ec`). It is test-only plus one exported validator: `isObjectiveKey` shared with `publishLab`; a property test that every objective key maps injectively to a valid Isoloom input name `CTF_FLAG_<KEY>`; a leak test that no flag or wrong answer is stored in the graph or logs; and a local-only e2e backend (a throwaway Neo4j container, keys and secrets generated per run, 127.0.0.1). Local main only, no push, no deploy, no shared database. cyberctf-web's opt-in e2e (`228d055`) is on main.

**The Cyber Courses website** (`cyber-courses-web`, Cyber-Courses/Cyber-Courses-Web, Next.js 16, pnpm; course catalogue and structure from CyberBackend, lesson MDX from the private content repositories, progress and skills, and the capstone lab hand-off). Maintainer (2026-10-16): "can we do the same for cyber courses". Same rules and the same playbook as cyberctf-web: local `main`, no PRs, no push without the maintainer's OK.
- **Running now:**
  - consolidation onto `main`: archive the branches; integrate the valuable ones, including open PR #6 (the lesson animation library); fix the package name copied from the CTF site; keep only pnpm's lockfile;
  - the engineering standard (quality ratchet to zero, folders by feature, typed boundaries for CyberBackend, the content loader and the session);
  - Vitest and Playwright (no tests today), and a security audit with pinned fixes.
- **Running now: the course content contract.** The path is content repo `course.json` and MDX → `cyber-course-sync` → CyberBackend → cyber-courses-web → the capstone CTF lab through the launcher → grading.
  - **Suspected so far:** the capstone `lab` block in `course.json` still uses the old single-evidence model (`evidenceKind`, `evidenceParams`), not objectives, and its `commit` is a placeholder.
  - **Where tests go:** cyber-course-sync gets the standard and typed schema tests; cyber-courses-web gets one opt-in e2e after consolidation.
  - **Proposed only, not edited:** changes to CyberBackend, the content repositories and the launcher.
- **Course content contract, audit findings (2026-10-16).** Every GraphQL document matches production's schema, so the mismatches are semantic.
  1. **The sync clobbers labs.** A course's `lab` block re-publishes the lab, which archives its objectives (the CTF per-objective flags break) and overwrites its runtime. Fix: a course references its lab by slug only; the sync never calls publishLab.
  2. **A placeholder lab is in production.** `invoice-portal-sqli` points at a repository that doesn't exist, at a placeholder commit, as a DRAFT the course contains.
  3. **The lesson and the graph point at different labs.** The SQLi lesson embeds `invoice-portal-api`, an API-reading lab, while the graph's capstone is `invoice-portal-sqli`.
  4. **No grading from a course.** cyber-courses-web has no flag or objective submission at all.
  5. **The launcher drops flags.** The `launch_env` bug hits course capstones too; the CTF fix covers it.
  6. **Security, live in production:**
     - **The flaw:** the lesson fetch builds a GitHub API URL from unvalidated `contentRepo`/`contentPath`, so a publisher credential can make the public site render private repositories.
     - **Fix:** an owner allow-list, strict path and commit-ref rules, and installation tokens scoped to one repository.
     - **Delivery:** prepared as an unpushed hotfix branch plus `main`.
  7. **Authorization.** One `publisher` role covers lab and content repositories, with no per-repository scope.
  8. **Refs can float.** Outside CI, refs default to `main`. Fix: require a commit SHA in the sync and the web.
  9. **Progress is fakeable.** It rests on client-reported `correct`, with no dedupe, so mastery can be farmed. `teachesSkills` is unused, and `useProgress` calls a missing route.
  10. **Small drift** in the web.
- **Owners:**
  - the sync (1, 8, plus the cross-checks): its worker;
  - the web (6, 8, 10): the cyber-courses-web worker;
  - **waiting on the maintainer:** 2 and 3 (content, plus a production admin action), 4 (embed or link out), 7 and 9 (CyberBackend and Cyber Auth), and the backend halves of 6 and 8. The proposals are written in the scratch file `ccontract-proposals.md`.
- **Later:** GitHub cleanup of Cyber-Courses-Web (PR #6 and the remote branches) once consolidated; cyber-course-sync is never pushed without the maintainer, because content repos run it from GitHub. The iOS app (`cyber-courses-ios`, a SwiftUI scaffold on sample data) is not in scope yet.
- **Later:** a sync that shows each launcher mode's real tiers (from Isoloom's `tiers.json` and `docs/matrix.md`), and where each lab runs, after the lab rewrite. The launcher page and docs copy come at the docs gate.

**When:**
- **Phase 0.7, the engineering standard in both sites:** measure, then lint configs, the ratchet, the folder layout, and driving to zero. It can run alongside other phases, because the sites don't block the converter.
- **Phase 2:** the Isothrum compiler (`isothrum-core`, soon `isothrum-compiler`) and the website editor **write the new spec syntax**. The editor's model must accept exactly what Isoloom accepts. For example, a rate with no unit is reported, as Isoloom does; today the editor reads it as bits. With no old-format support, a drawn lab would otherwise break. They test against Isoloom's spec examples and schema.
- **Until Phase 7:** isoloom.com carries a "1.0 is being rebuilt" banner (live since 2026-10-15, commit `60098c4`), because its pages still describe the 0.x preview. The banner is removed in Phase 7.
- **Phase 7, release:** isoloom.com's docs are regenerated from the repository's new docs, and its pages rewritten (features, targets, comparisons) from the 1.0 facts. On isothrum.com: the editor on the 1.0 spec, the WASM from the published npm package instead of the committed `public/wasm/`, and the release announcement. Both are pushed with the 1.0 release.
- **1.x:** `desktop-editor/` and the editor components move into `isothrum-ui`, shared with the desktop app. Accounts (Better Auth, ex-isothrum #30) and the project store follow.

## 4. Work, in order

**Principles:**
- **The safety net comes before the surgery.** Every change is measured against characterization snapshots of the generated output and against the official validators.
- **Vertical first, then wide.** One real lab goes end to end through the new path before any breadth.
- **Lower, don't rewrite.** The new syntax lowers into the existing plan IR.
- **The launcher's needs decide what is in 1.0.** Everything else is 1.x.
- **Parallelize only disjoint work.** Targets and drivers can run in parallel; the spec and the plan core are serial.
- **Close each phase cleanly.** Every phase ends green across all three repositories, with a short review of its own diff.

Sizes: S is about a day of focused work, M a few days, L about a week. There are four tracks; Tracks B and C start now, in parallel with A.

### Track C: start today (no code, long lead times)
- **Real infrastructure for server and cloud mode:**
  - a Proxmox host, and an ESXi one if possible;
  - budget-capped accounts: AWS first, then Azure and GCP.

  Without them, server and cloud mode ship validated but not proven.
- **Reserve names:** the crates.io names `isoloom`, `isoloom-*` and `isothrum*`, and the npm scope.
- **Trademark filing** for Isoloom and Isothrum, if branding matters.
- **The CTF lab specs:** every lab spec from the Cyber-Courses and CyberCTF repositories, copied locally (private) as reference material only, with a report of which features the labs use. It tells the new syntax what to express first; the files are rewritten at the end.

### Phase 0: The safety net (M)

1. **Characterization snapshots.** Generate every example on every target from today's `main`, and snapshot the output with `insta`. Any later change that alters output must be deliberate and reviewed.
2. **(Dropped.)** Old specs are not a regression suite, because the old format isn't supported.
3. **`verify`, the seed version.**
   - **One pinned toolchain image** with every official validator:
     - `docker compose config`;
     - terraform plus tflint;
     - vagrant;
     - kubeconform and helm;
     - cloud-init;
     - hadolint;
     - ansible-lint;
     - shellcheck;
     - pwsh with PSScriptAnalyzer.
   - **Behaviour:** it runs over the snapshots. The image runs the same on macOS, Windows and Linux.
   - **Reuse:** salvage the generate-everything pipeline from `v1/security-scanning`.
   - **Baseline:** failures found now become a baseline of existing problems, fixed in later phases rather than ignored.
4. **Local multi-repository setup.** Clone `isoloom`, `isothrum` and the launcher side by side, with `[patch]` path dependencies. One `check-all` script runs every repository's checks.
5. **Cross-platform check (decided).** A manual-only GitHub workflow that runs `check-all` on macOS, Windows and Linux on demand. No PRs and no automatic runs. Without it, Windows and Linux bugs surface only after the squash.

6. **The engineering standard in the launcher:** lint configs at the 3b levels, the ratchet, the dependency-direction check, `check-all` wiring, the folder reorganization (everything except `runtime/`), and the non-runtime code driven to zero violations.

**Exit:** snapshots and `verify` are green or baselined, and `check-all` passes.

### Phase 1: Subtract (S–M)

Now safe, because the net is in place.
1. **Delete the runtime from Isoloom.** It stays on a salvage branch. The runner takes from it what it needs in Phase 2; nothing is copied into Isothrum blindly.
2. **Delete `isoloom-coverage`.** Its schema checks are covered by `verify`, and its capability declarations stay in the backends.
3. **Delete the committed example outputs.** The snapshots replace them.
4. **Delete the compatibility debt** and fix the small debts: the Ruby-dependent test, the dead-code fixtures.
- **Keep `isoloom targets` (and its refusal reasons) working without the runtime crate.** The characterization snapshots call it (today it reaches into the runtime for `--host`).
5. **Apply the engineering standard to Isoloom:** the repository layout (3b), `xtask` replacing `scripts/*.sh`, the dependency-direction test, `rust-toolchain.toml`, and the ratchet **driven to zero**. Every move is proven output-neutral by the snapshots.

### Phase 1c: Deep organization and clean code, no shortcut (maintainer, 2026-10-15)

Phase 1 is done only when every Isoloom crate is deeply organized and has **zero** violations. Grouping the crates isn't enough.

1. **The strictest lint set, enforced at deny,** across the workspace:
   - **Exceptions need reasons:**
     - `clippy::allow_attributes` and `allow_attributes_without_reason`: every exception is `#[expect(lint, reason = "…")]`, which also fails once it's no longer needed;
     - `#![deny(warnings)]` in CI terms, through `check-all`.
   - **Rust and clippy:**
     - `unreachable_pub`: the public API is minimal, `pub(crate)` by default;
     - `missing_docs` on every public item, and `clippy::missing_errors_doc` / `missing_panics_doc`;
     - clippy `pedantic` at deny.
   - **Clippy restriction rules:**
     - `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` and `dbg_macro`;
     - `print_stdout` and `print_stderr` in libraries;
     - `indexing_slicing` and `string_slice` in libraries;
     - `as_conversions`, `wildcard_imports`, `shadow_unrelated`, `let_underscore_must_use`, `clone_on_ref_ptr` and `rc_buffer`;
     - `str_to_string` and `get_unwrap`.
   - **Limits:** `too_many_lines` 60, `too_many_arguments` 5, `cognitive_complexity` 15, `fn_params_excessive_bools`, `struct_excessive_bools`, and files ≤ 400 LOC.
2. **Organization inside every crate,** reviewed against one template:
   - **Module layout:** `lib.rs` holds only the module tree and the re-exported public API. One concept per module, and folders by domain concept, never by kind (no `utils.rs`, `helpers.rs` or `misc.rs`).
   - **Errors:** one error enum per crate in `error.rs`, with the CLI mapping codes to exit codes in one place.
   - **Tests:** unit tests next to the code (`#[cfg(test)] mod tests` in the same file, or `foo/tests.rs`), integration tests in `tests/`, fixtures in `tests/fixtures/`, snapshots beside their test, and shared test helpers in one `testing` module (`#[cfg(test)]` or a dev-only crate).
   - **Names:** the 3b vocabulary everywhere; no stutter (`isoloom_spec::spec::SpecSpec`); the same word for the same concept in every crate.
   - **Dependencies:** each crate lists only what it uses (machete); the dependency-direction test passes.
3. **The 3d tooling,** wired into `cargo xtask` and `check-all`: nextest, cargo-hack feature powerset, msrv verification, cargo-deny bans and sources, doc lints at deny, fuzz targets, cargo-mutants on core, criterion benchmarks with a stored baseline, and the generation-time budget test.
4. **Ratchet every count to zero, crate by crate,** with real refactors only. This runs in parallel by crate group: backends, then targets, tools and cli, then core, once the spec front end has landed in core.
5. **A principles and 3d review per crate,** using the 3b table and the 3d API, errors, docs and testing items as a checklist: SRP, DRY, KISS/YAGNI, CQS, Demeter, boundaries, F.I.R.S.T. tests and names. The review notes every finding with its fix, and the phase closes when every finding is fixed.

**Exit:** green net (snapshots byte-identical, `verify` with no new failures); `quality-baseline.json` at zero for every crate; every surviving mutant in core killed or listed with its reason; fuzz targets clean; every `#[expect]` carries a reason; each crate's review checklist closed; all of it on three hosts (`check-all`).

### Phase 2: The walking skeleton (M–L)

One real lab, end to end: the new spec, Isoloom, run manifest, runner, launcher.
1. **The new spec front end:**
   - a machine has `os`, plus `app:` (a custom image or build recipe; CTF labs need this first) or `service:`;
   - the shared fields;
   - `overrides:`.

   It lowers into today's plan IR.
- **Design input from the 376 CTF lab specs** (`~/code/isoloom-corpus/corpus-features.md`):
  - **Containers:** 344 labs are container-only, and 409 of 479 container machines build from a local folder. So `app:` with a build recipe comes first.
  - **The flag pattern:** 330 labs repeat it (an input, an init script, a named volume). It deserves a first-class, generic construct, such as an input materialised into a file at start-up.
  - **Networks:** every lab has one network, and 327 have no internet.
  - **Checks:** 822 in all (http, script, tcp, with `wait`).
  - **VMs:** 19 VM labs, mostly Windows and GOAD-style, run with an Ansible controller.
  - **Messages:** 371 reference port fields by path, so the new names must be stable.
2. **Rewrite by hand** the examples and that one lab (invoice-portal-api) in the new syntax. Old `docker:`/`vm:` blocks become `app:`, with one recipe both forms derive from.
3. **Run manifest v0 for the `docker` target:** commands, readiness, outputs and check runners. Its JSON schema and fixtures live in Isoloom, and both repositories test against them.
4. **The runner skeleton** in `isoloom/isothrum`: the manifest executor and the Docker driver, extracted from the launcher's runtime (its most proven code).
5. **The launcher** runs one real lab (invoice-portal-api) through Isoloom and the runner, behind a switch. Its real-lab test passes.

**Exit:** that lab passes end to end through the new path. The contract is validated by its real consumer.

### Phase 2 design, decided (2026-10-15)

The full design is in `plan/phase2/`: `run-manifest-v0.md`, its JSON Schema, `runner-extraction.md`, and two worked manifests. Its open questions are settled as follows.

**Run manifest:**
- **Format:** one `run.json` per target, holding the native commands; drivers hold only documented actions. Strict (`additionalProperties: false`) until 1.0, then only `x-` extensions.
- **Run name:** the embedder sets it (`cyberctf-<id>`); the CLI defaults to `lab.name`.
- **Re-provision:** the embedder passes inputs again; the runner stores no secret values.
- **Generated files are host-neutral:** `local-exec tar` becomes a `project.archive` with a `tf_var`; `.sh` helpers become module outputs or steps; `cloud.requires` uses a per-OS `host_argv`.
- **Check timeouts:** Isoloom computes them (waits + 15 s per check + 60 s), so the manifest is self-contained. This fixes the launcher's 5-minute cap that kills long-waiting labs.
- **Docker park:** stays `stop` (no `pause`, which keeps memory).
- **Vagrant:** `VAGRANT_DOTFILE_PATH` moves into the run's state in Phase 3.
- **Vagrant Proxmox provider:** Tier 2, not Tier 1. The launcher runs Proxmox through Terraform only, and the plugin is unmaintained.

**Runner:**
- **Crates:** `isoloom-manifest` (the types, the generated schema and the fixtures) in Isoloom; `isothrum-runner` (the library) and `isothrum` (the CLI) in Isothrum.
- **`unsafe`:** safe wrapper crates where they exist (`process-wrap`). The rest (UAC elevation, the Windows console, Ctrl+C) sits in one audited `sys` module, the only place `unsafe` is allowed.
- **Detached runs:** through the embedder's own executable, via a runner entry point, so there's no second signed binary.
- **Windows delivery:** Vagrant's own `winrm` and `upload`, plus the cloud secret stores. Python is used only for Proxmox and ESXi Windows machines, until OpenSSH-on-guest or a Rust client removes it. The goal is no host Python by 1.0.
- **Live cloud labs at the switch:** a one-time import into the runner's records, so players' running labs stay destroyable.
- **Homes:** separate for the launcher (its app data) and the CLI (`~/.isothrum`); the CLI can attach with `--home`.
- **Events:** the event stream has its own version.
- **The launcher's switch:** a hidden developer flag in Phase 2, then a visible setting once Vagrant lands.

**Runner follow-ups (decided 2026-10-15):**
- **Check grammar:** the `isoloom-check/1` parser moves into `isoloom-manifest`, so there's one source; the runner drops its copy.
- **Compiler lints:** the compiler and WASM crates take the workspace lints in the compiler-on-new-syntax unit.
- **State files:** the runner is synchronous, so small state files are read and written inline. Documented in the crate docs; that is 3d's "chosen once per crate".
- **Isothrum's 3d tools** (cargo-deny, cargo-hack, msrv, semver-checks, fuzz, mutants) are wired in Isothrum's own 1c-equivalent pass.
- **`cargo xtask dev-link relock`:** updates a consumer's lockfile from the local Isoloom without GitHub, because the pinned revisions aren't pushed.

**Bugs found in today's code**, fixed by the manifest and runner work (`runner-extraction.md` §6):
- Launcher and Isoloom names have drifted: the port labels, `address` vs `ip`, the ready file.
- The Proxmox `slot` is never set.
- Windows inputs are dropped on Vagrant.
- `wait_ready` reports OK when SSH never answers.
- The lab lock fails open.
- A destroy uses the current region instead of the applied one.
- There are panics outside tests.
- The launcher pins `isoloom-core@50147b3`, which is reachable only on GitHub's object store, since the `dev` branch was deleted; its archive is in `~/code/github-archive/`. The path-dependency switch in Phase 2 removes the pin.

**Syntax: approved by the maintainer (2026-10-15),** exactly as designed in `plan/phase2/spec.md`:
- `os` plus one of `service:` or `app:`, with `overrides:` keyed by target;
- `app.steps` is portable, while `app.build` and `app.image` are container-only;
- ports are a map keyed by name, with one `protocol` enum;
- the flag is a value delivered through `files:` with `input:` or `secret:` as its source;
- `setup:` replaces `docker.init`;
- coded errors.

Its §12 open questions take the design's recommendations.

**More decisions (2026-10-15):**
- **Vagrant check timeouts** use the same rule as docker (waits + 15 s per check + 60 s); the controller's boot is a ready condition, not check time.
- **Launcher `config`** moves from `app/` to `platform/`, so domain code never imports the app shell.
- **Launcher `tauri` imports in domain code** reach zero when `runtime/` moves to the runner (Phase 2–3).
- **Launcher `unsafe`** follows the runner's rule: one audited `sys` module.
- **ESXi** server mode is supported only through SSH + ovftool (Vagrant `vmware_esxi`, Terraform `josenk/esxi`); no vSphere API on the free edition. The launcher uses Vagrant for ESXi.
- **ESXi finding:** the Terraform output reports the router's lab-side address when its uplink gets no DHCP. Fix it in Phase 3.
- **`tuned-web` keeps its TPM** (`hardware.tpm: required`), so it is VM-only: the example exists to show hardware tuning.
- **ESXi finding:** ESXi 8 refuses ed25519 keys in FIPS mode. The launcher's guidance must say so, or use RSA.

### Phase 3: The launcher's modes, wide (L, parallel per target)

Each step means: lowered generation passes the snapshots and `verify`, then a manifest, then a runner driver, then the launcher's lab test.
1. **Local:** `vagrant` (VirtualBox, Hyper-V, VMware, QEMU/libvirt) and `docker-vm`.
2. **Server:** `proxmox` (Terraform) and the Vagrant ESXi provider, with the launcher's `connection_env` contract. The Vagrant Proxmox provider is Tier 2.
3. **Cloud:** `cloud-vm`, `cloud-docker` and `cloud-services`, AWS first and then the other clouds.
   - **Proxmox bugs found by the real smoke test** on `pve-lab` (2026-10-15), to fix in this phase:
     - `destroy` leaves the SDN zone and bridge pending, because the "apply SDN" resource is destroyed before its zone; it needs the provider's finalizer ordering.
     - The module defaults to `node = "pve"`, while the launcher's Test connection uses the first node. A server with another node name passes the test and then fails the launch. Default to the discovered node, or make it required.
     - A router whose guest agent can't install (no DNS or internet) hangs `apply` for 15 minutes. Fail fast with a reason.
   - **ESXi findings from the real smoke test** (`esxi-lab`, 2026-10-15), to fix in this phase:
     - **The Vagrant controller on `vmware_esxi` never reports its address,** because the `generic/alpine319` box's VMware Tools don't report one, and `vagrant up` times out. Decided: use `bento/debian-12` for the controller on `vmware_esxi`, as on ARM.
     - **Terraform `esxi` can't finish natively on macOS.** The generated `terraform_data` step makes Terraform SSH to the router's DHCP address, which Local Network Privacy blocks. The container can reach it, but has no Linux `ovftool`. Decided: guest SSH steps move out of Terraform into runner manifest steps (`ssh.run` through the runner's relay or `nc`, as the launcher already does for Vagrant). Terraform keeps only the infrastructure. This also makes the step host-neutral.
     - **With promiscuous mode on,** the router gets its uplink by DHCP. The fallback that reported the lab-side address (10.80.0.254) must still turn into a clear failure, not a wrong output.
   - **Tier 1 for server mode** needs runs from Linux and Windows hosts too. `pve-lab` and `esxi-lab` sit on VMware Fusion's private NAT (172.16.1.0/24), which jarvis and karen can't reach. Track C task: move both lab VMs to **bridged networking on the LAN** (static 10.0.0.x addresses, reserved on the router), so the runner can drive them from jarvis and karen. Then record `proxmox` and `vagrant/vmware_esxi` from both hosts.
   - **`cargo xtask tiers record` doesn't deduplicate:** recording the same run twice lists it twice. Make it idempotent (same entry, host, date and server means an update, not a new row), with a test.
   - **Cloud design (approved 2026-10-16):**
     - Terraform keeps only the infrastructure. Each machine sets itself up from its own user data, and the runner delivers the project over `ssh.run` stdin. This replaces the earlier "`project.archive` with a `tf_var`": Terraform no longer reaches the guest, and user data is capped at 16 KB.
     - `ssh.run` gains `with.wait_s` (retry only while ssh can't connect).
     - **Secrets:** each cloud's store gets one secret per value, through a write-only attribute fed by an ephemeral variable, so the value is in neither a generated file nor the state (Terraform ≥ 1.11). The machine reads its secrets at boot through its identity. DigitalOcean, Linode and **OCI** fall back to guest delivery over SSH (the runner writes the values into `/var/lib/isoloom/secrets` and setup waits for `.delivered`). **OCI decision (2026-10-16):** OCI Vault is not used, because a vault's deletion is scheduled 7 to 30 days out (so every lab would leave one behind, against a per-tenancy cap), reading a secret through the instance identity needs a dynamic group and policy at the tenancy root that a lab's compartment and most player accounts can't hold, and instance-principal request signing isn't practical from a boot script. The fallback keeps values out of every generated file, state, event and log, and its SSH delivery runs before the egress seal.
     - **The runner** strips ambient cloud credentials from Terraform steps (destroy replays the recorded profile and region). It adds `isothrum reap <cloud>` and a credential-guarded `real-run cloud`.
     - **The launcher** gets the one-time import of its old deployment records.
     - **Windows on cloud-vm** sets itself up from its user data (PowerShell), or keeps Terraform's WinRM provisioner, as a reported gap, if that runs long.
   - **Bugs from 6a's `tf-test`** (each recorded as an expected failure, so its fix flips the suite):
     1. **Azure doesn't enforce `reach` between networks** (Azure's default `AllowVnetInBound` lets every subnet reach every port). This is the most serious: segmented labs on Azure aren't segmented. Owner: the cloud worker.
     2. **`internet: false` is enforced only inside the guest,** where root can lift it. **Decided by the maintainer (2026-10-16): "seal after setup."** Machines get internet only during setup. Once ready, the runner applies a seal step at the network level: no public IPs, default routes or NAT for `internet: false` networks; on edge-firewall, `lan` only through `fw`; Proxmox and ESXi routers stop forwarding those networks. Root in a guest can't undo it. Owners: the cloud worker (clouds), then a server follow-up (Proxmox and ESXi routers).
        - **How the seal works (refined 2026-10-16):** the seal is egress-only, enforced at the network level, and applied by the runner (`terraform apply -var sealed=true` after ready). Security group or firewall egress is limited to the lab's ranges, and `internet: false` subnets lose their internet-gateway route. The literal "no public IP" would cut the runner's SSH and the published ports.
        - **Contract (2026-10-16):** run manifest v0 gains an optional `Lifecycle.seal: Vec<Step>`, left out when empty so other manifests stay byte-identical. It runs once up has succeeded and the ready conditions hold. The seal is a second apply with the module variable on, never an out-of-band change, so a later apply can't silently undo it. "Sealed" is recorded in the run's state, so every later up and start applies with it on. It fails closed: a failed seal means the run is not ready. The Proxmox and ESXi routers reuse the same field later. **Runner side landed `isothrum@07e07fc`:** seal fail-closed with replay on every later op and forgotten on down; terraform.copy_module; tests for all three. 284 runner tests green.
        - **Residual channel, DNS:** the cloud resolver recurses to public names. It's closed with a DNS allow-list of the lab's own names where cheap (Route 53 Resolver DNS Firewall, GCP response policy, Azure DNS Private Resolver, OCI resolver rules), and documented as a known limitation otherwise.
     3. cloud-docker has no `published` output keyed `<machine>/<port-name>`, on any cloud. Owner: the cloud worker.
     4. Proxmox private-names: a VM's hosts file leaves out its own names. Owner: a server follow-up.
     5. Not plan-testable: ESXi writes only `ready` (no `running:`/`failed:` progress); DigitalOcean can't pin a private address (best effort; document it); Linode private names live only in the hosts file; Terraform 1.16.6 crashes in `terraform test -json` when a failing assertion reads OCI's `route_rules`.
   - **Brought back through the runner** (removed with Isoloom's runtime in Phase 2): the cloud-services `cloud.requires` pre-check (as the runner's tool probes), the launcher's Machine page per-target readiness (from the manifests' `tools`), and Settings' lab list (from the runner's run store).
   - **Retiring the launcher's old Docker path:** its resume still passes empty inputs, a patch that disappears when the runner path becomes the default.
4. **Secrets conversion and native checks** for these targets only:
   - Compose secrets;
   - Vagrant host environment;
   - each cloud's secret store, AWS first;
   - checks as native artefacts.
5. **Port the rest of the launcher's runtime into the runner,** driver by driver: locks, park and resume, recovery, reaper, budgets, remote servers, Isoloom's secrets delivery.

**Exit:** every launcher mode runs its test labs (rewritten in the new syntax) through Isoloom and the runner. Local is proven on macOS, plus Windows and Linux through the cross-platform check. Server and cloud are proven where the Track C infrastructure exists.

### Phase 4: The catalogue and the CM model (L)

**Design approved by the maintainer (2026-10-15),** with all 13 recommendations: `plan/phase4/design.md`, the examples, and the nginx example entry.
- **The model:** eight new step kinds (`repository`, `file`, `template`, `user`, `group`, `service`, `command` with a required guard, `firewall` allow-only) join the approved five. Ordered and guarded; downloads pinned by SHA-256 or digest.
- **Rendering:** one POSIX shell library for Dockerfile, shell and cloud-init; Ansible with official modules only, from the Linux controller; a PowerShell library for Windows. The VM set-up splits after the secrets step (the IaC tool before, the CM tool after). Defaults keep today's output; `--configure` plus a per-target override chooses the CM tool.
- **Services in containers:** one daemon runs as the container's foreground process; several run under the engine's init (tini, Compose `init: true`) with a generated runner; refusal only when a daemon has no known foreground form. No s6-overlay in 1.0.
- **The catalogue:** a hand-written `entry.yml`; a `lock.yml` (digests, exact package versions) written only by `cargo xtask catalogue refresh`. The official image is used only when its OS matches the machine's; otherwise the same recipe builds from the OS image. GPL Galaxy content is referenced, never embedded, and no vendored Galaxy content in 1.0. The folder is `files/`, not `roles/`. Version prefixes (`postgres@16`) resolve to the newest 16.x. Old versions come from the OS that shipped them (MySQL 5.7 on ubuntu@18.04); vulhub-style images stay `app.image`.
- **Entries, wave 1:** openssh, nginx, apache, php-fpm, mysql/mariadb, postgres, **mongodb**, dvwa, juice-shop. Then wordpress, redis **and valkey** (Redis 7.4+ isn't open source), rabbitmq, iis, ad-ds.
- **Not in 1.0:** a `start` phase on `app`; several catalogue services on one container stay refused.
- **Parity:** proven on Docker plus Vagrant/VirtualBox through the Isothrum CLI first, then libvirt on jarvis.

1. **The configuration resource model** (repository, package, file, template, user, group, service, command, firewall rule). `app:` build recipes are written in it, so the container build and the VM install come from one source.
2. **CM backends:** Ansible, cloud-init, shell/PowerShell and Dockerfile, with the pairing rules.
3. **The catalogue engine,** salvaging from `v1/catalogue-engine`. GPL Ansible collections are referenced, never embedded.
4. **Entries, CTF-relevant first:** openssh, nginx, apache, php-fpm, mysql/mariadb, postgres, dvwa, juice-shop. Then the rest: wordpress, mongodb, redis, rabbitmq, iis, ad-ds.

**Exit:** each entry passes its parity contract for real, in both forms, on Docker and one VM provider.

### Phase 5: The other targets (M, parallel)

Kubernetes, Helm, Kustomize, hybrid, hosted, external and Terraform ESXi, onto the new spec through lowering. They are Tier 2: snapshots plus `verify`.

### Phase 6: Hardening (M)

1. **Combinations:** property-based random specs; every target's output must pass `verify`.
2. **Cloud logic without clouds:** `terraform test` with mock providers for routing, gateways, firewalls and per-machine ports.
3. **Tiers from evidence,** and `isoloom matrix` generated from the backends' capabilities.
4. **Security review** of secrets and of everything that handles credentials, in Isoloom, the runner and the launcher.

### Phase 7: Release 1.0, together (M)

1. **Minimal docs** in each repository: getting started, the generated spec reference, targets, secrets. Then the two websites (3c): isoloom.com from the new docs, and isothrum.com on the 1.0 spec and the npm WASM.
2. **The docs alignment gate.** No release until every doc matches the code, in every repository and both websites:
   - **Generated** from the code, with stale-file tests: the spec reference, the targets and capability matrix with tiers, the catalogue, the CLI reference, the manifest schema, and the error codes.
   - **Executed:** every code block in the docs (specs, commands) runs in a test. Specs pass `isoloom validate` and `verify`; commands run against the examples. A doc example that stops working fails the build.
   - **Audited:** every hand-written claim (READMEs, guides, comparisons, both websites' pages, each repository's `AGENTS.md`/`CLAUDE.md` and contributor files) is checked against `plan/docs-facts.md`, the tests and the tier evidence. No claim without proof: "supported" means Tier 1 or Tier 2 with the evidence named.
   - **One source:** isoloom.com's docs come from the repository through `sync:isoloom`; nothing is edited only on the site.
   - **Exit:** an audit checklist covering every doc file, each item closed.
3. **Squash each repository's history,** with the maintainer's confirmation at that moment, and re-enable CI.
4. **Publish to crates.io:** the Isoloom crates, then the Isothrum runner.
5. **Rewrite every CTF lab file in the new syntax** (by hand, with each lab's real run as proof).
6. **Switch the launcher from path dependencies to the published versions,** and release the launcher.

**Exit:** the launcher runs its full catalogue in local, server and cloud mode on published Isoloom 1.0 and the runner.

### Phase 8: After 1.0 (1.x)

- **Isothrum:** `isothrum-desktop` (moved out of `apps/desktop`, dynamic specs) and `isothrum-ui`, shared with the website and the launcher.
- **Isoloom ecosystem:** import as the front door (today only Compose, Vagrant and Terraform import; **Helm and Kubernetes importers don't exist yet** and are built in 1.x), the VS Code extension, packages, registry modules and the website rewrite.
- **More CM and IaC tools:** Puppet, Chef, Salt, DSC, Pulumi, Podman.
- **Windows on container targets** (dockur), from `v1/windows-in-docker`.
- **Runtime features in Isothrum:** drift detection, upgrades, snapshots and the hub.

## 5. Open decisions

1. **CTF lab content and courses licence.**
2. **Does the launcher use `isothrum-ui` (1.x)?** Recommended: yes.

Decided on 2026-10-15:
- The launcher is integrated from Phase 2.
- There is a manual-only cross-platform workflow.
- Track C infrastructure:
  - **Proxmox VE 9.2 (`pve-lab`)** runs in VMware Fusion at `172.16.1.10`. It is proven: a generated lab applied, served and passed its check, then was destroyed.
    - Credentials are in `~/.isoloom-lab/proxmox.env`.
    - Terraform runs in Docker (`~/.isoloom-lab/bin/pve-tf`), because macOS Local Network privacy blocks provider binaries started from the agent session.
    - Lab VMs use the internal NAT bridge `vmbr1`.
    - It's stopped when idle.
  - **ESXi 8.0U3e** runs as a VM in VMware Fusion (installing).
  - **Windows host:** `florianamette@10.0.0.40` (`karen`) over SSH key, with Windows 11, 16 GB RAM, Hyper-V, Rust MSVC, Node 24 and Docker. It gives the Windows host proof and Hyper-V.
  - **Linux host:** `florianamette@10.0.0.10` (`jarvis`) over SSH key, with Debian 13, 46 GB RAM, KVM, libvirt, Docker and Vagrant. It gives the Linux host proof, and real Vagrant runs on libvirt and QEMU.
  - Cloud accounts come later.

## 6. Salvage branches (local, not pushed)

| Branch | What may be reused |
|---|---|
| `v1/catalogue-engine` | Catalogue format, loader, nginx entry start, Alpine 3.24 image table entry |
| `v1/windows-in-docker` | dockur container form, KVM refusal, OEM set-up |
| `v1/security-scanning` | generate-everything pipeline, scanner baselines, secure-default generator fixes |
| `v1/cli-output-compat` | spec table as JSON (the matrix may reuse it) |
| `v1/docs-truth-fixes` | Google Cloud MTU capability cell |

They are deleted at the squash.

## 7. Backlog carried over from GitHub (closed 2026-10-15)

Every issue and PR was closed, and every non-default branch deleted, for the fresh start:
- **Full archive:** `~/code/github-archive/`, one verified git bundle per repository (every branch, tag and PR ref), plus the issue and PR texts as JSON.
- **Kept on purpose:**
  - the launcher's releases and tags, because the installed launchers' updater reads `releases/latest/download/latest.json`;
  - `isoloom/goad`, a fork of GOAD whose branches and tags are upstream's.

| Item | Carry into |
|---|---|
| launcher #227 | Starting a lab always needs sign-in and the backend: allow offline relaunch and/or a practice mode |
| launcher #225 | Windows dev setup: CRLF checkout fails pnpm check and dirties generated types, sh-based tests fail, local build needs the signing key |
| launcher #217 | Windows: in-app terminal can't SSH to remote attack boxes (cmd /c quoting); shell scripts break on non-ASCII user names |
| launcher #178 | Own the Isoloom integration in the launcher |
| launcher #150 | Clean code: split large files and long functions |
| launcher #79 | Isoloom and lab images: QEMU x86 emulation, networking and Windows boxes |
| launcher #78 | Run x86_64 VM labs under QEMU emulation on Apple Silicon |
| launcher #2 | Adopt Isoloom's lifecycle, checks and defaults features |
| isothrum #30 | Accounts with Better Auth: Isothrum website (proprietary), after 1.0 |
| isothrum PR #65 | Isoloom v1 crates in Isothrum: superseded by the path-dependency setup |
| isothrum PR #66, isothrum-website PR #18 | Editor and engine: IPv6 LANs, a machine's environment, files and command. Re-apply on the editor in 1.x, from the archive |

Placement of the launcher items:
- #2, #178, #78 and #79 fold into Phases 2–3: the Isoloom integration, the runner, and QEMU and Windows boxes.
- #150 folds into the engineering standard: the ratchet.
- #217, #225 and #227 are launcher work for Phase 3 and the release.
