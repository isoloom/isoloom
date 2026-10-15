# Review: isoloom-render (Phase 1c)

What every backend renders with: the `Backend` port and its `Error`, the builders (HCL, Ruby,
PowerShell, Dockerfile, the Ansible inventory), a VM's set-up steps, the network pieces and the
containers' shared parts. One line per 3b principle and 3d item: `ok`, or the finding and its fix
commit.

## 3b design rules
- Functional core, imperative shell: ok (every function is pure over the plan; the `.isoloomignore` matcher no longer reads or walks the project: `Rules::read` and `entries` went, the one test that walked a project does it itself over `Rules::parse` and `Rules::ignored`).
- Ports and adapters: ok (`backend::Backend` is the port every backend implements; nothing here runs a tool).
- One-way dependencies: ok (spec, plan and manifest only; the backends and `isoloom-targets` depend on it, never the reverse).
- Parse, don't validate: ok (a trunk's VLANs are a typed `Vlan`, not `(String, u16, u8)`; a trunk's end is `End`, not `0`/`1`; how a kept script reruns is `boot::Trigger`, not a `bool`; how a module copies the project is `project::Copy`, not a `bool`; an OS family's installer is a private `Installer`; the proxy's URLs are `(http, https)` once read).
- Typed errors: ok (one `Error` in `error.rs`, `Refused { target, refusal }` and `Unsupported { target, what }`, with `target()` and `reason()`; `GenerateError`, the registry's error, moved to `isoloom-targets`, which owns `NotPossible` and `NoGenerator`; the HCL builder surfaces `hcl-rs`'s own error, typed, as `hcl_builder::Error`).
- No hidden state: ok.
- Determinism: ok (ordered maps throughout; the rendering-twice test of the inventories).
- One way to do each thing: ok (one PowerShell quote: `steps::windows` had a weaker copy of its own that let a typographic quote end a string, a bug, fixed with its test; one Windows project folder constant instead of three; one `trunks::of_machines` behind the container and VM trunks, the `dup/clone`; one controller script, `controller::provision_script`, instead of `ansible_runs` and `provision_script`; the controller's size is the plan's `Controller::of`; the plan's addresses are imported from `isoloom_plan::addressing`, not re-exported at render's root).

## 3b principles
- Names, one vocabulary, no stutter: ok (`trunks::trunks` became `trunks::containers` beside `trunks::vms`; `playbooks` became `controller`; `machine_setup` became `steps::settings`, `machine_files` `steps::files`, `disks` `steps::disks`; `hcl`/`rb`/`ps` became `ruby::quote` and `powershell::quote`).
- Small functions: ok (all under 60 lines, complexity under 15).
- Few arguments, no flag arguments: ok (`boot::at_boot`'s `on_link_up: bool` and `project::lists`' `archive: bool` became enums).
- Single responsibility, small modules: ok (by concept: `backend`, `error`, `output`; the builders `hcl_builder`, `ruby`, `powershell`, `dockerfile`, `inventory`; a VM's set-up `steps` (with `disks`, `files`, `network`, `process`, `settings`, `shared_volume`, `wait`), `boot`, `secrets`, `constraints`, `controller`, `project` (with `ignore`); the network pieces `router`, `trunks`, `ipv6`, `more_addresses`; `container` (with `jobs`, `project_files`, `host`, `hybrid`); `run_manifest` (the pieces every target's run manifest is built from); `checks`. Every file is under 400 lines of code.)
- Open/closed, Liskov, interface segregation, dependency inversion: ok.
- DRY: ok (no `dup/clone` left).
- KISS and YAGNI: ok (dead API removed: `host_vars`, `common_unsupported`, `container_checks_unsupported`, `router_name`, `as_target`, `route_commands`, `sudo_command`, `project::terraform`, `hcl()` (only tests used it), `Rules::read` and `entries`; Terraform-only helpers moved to Terraform: `arm64_machine`, `gcp_project_name`; the Vagrant controller's box moved to Vagrant).
- Command-query separation, no hidden side effects: ok.
- Law of Demeter: ok.
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic`, indexing or string slicing outside tests; a network or machine the plan guarantees is looked up with `get`).
- Clean boundaries with third-party code: ok (`hcl-rs` behind the HCL builder).
- Clean tests (F.I.R.S.T., one concept each): ok (unit tests next to the code with one `testing` helper; integration tests return a `Result` instead of allowing `unwrap`).
- Comments say why: ok.
- Formatting: ok.
- No dead code: ok (`unused` at deny; machete clean, its `hcl-rs` exception gone).

## 3d Rust practices
- Workspace hygiene: ok (`thiserror` from the workspace).
- Common traits: ok (`Error`: `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`, `std::error::Error`; `Trunk`, `Vlan`, `HostSize`, `HybridContainers`, `Rules`, `Template`, `Module`: `Debug`, `Clone`, `PartialEq`; `Access` has a `Debug` of its own, its host closure left out).
- Conversions through `From`: ok (`From<&str>`/`From<String>` for `Template`, `From<Template>` for `Expression`; `isoloom-targets` converts `Error` into its `GenerateError` with `From`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Error`, `Access`, `inventory::Host`, `HostSize`, `Trunk`, `Vlan` and `project::Copy` are non-exhaustive; `HybridContainers` is built by Compose, so it stays open).
- Ownership: ok.
- `#[must_use]`: ok.
- Visibility: ok (`pub(crate)` or `pub(super)` everywhere but the API; no `unreachable_pub`).
- Library errors with `thiserror` and a `Result` alias: ok (`Result<T, E = Error>`; a refusal has no source: its reason is its field).
- Error messages: ok (lowercase, no trailing period; every refusal's text unchanged).
- No panics in library code: ok (`format!` of an `hcl-rs` expression relies on `hcl-rs`'s documented infallible formatting; everything else is total).
- Sync or async, observability: ok (synchronous and pure; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok.
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (the `.isoloomignore` matcher; shell, Ruby, PowerShell and the HCL builder's strings and templates, shared with the `quoting` fuzz target).
- Snapshots only for whole files: ok (the six inventories).
- Mutation testing: ok (see "Mutation testing" below: every mutant caught, none listed as equivalent).
- Fuzzing: ok (the `quoting` target, 300 s, clean; see below).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok (no grab-bag left: the old root's helpers each went to their concept or their only user).
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok (`tests/quoting/literals.rs` is shared with the fuzz target).

## Mutation testing (`cargo mutants`, lints capped, 120 s timeout floor)
- Before (the earlier run, lints not capped, on the code before the spec switch): 715 mutants, 145 caught, 111 missed, 15 timeouts, 444 unviable. With lints capped, a partial rerun of that code (76 mutants) missed 51, mostly in dead API.
- The timeouts were loops with no provable bound under mutation: the ignore matcher's glob reader (`i += 1` mutated), its `*` loop, and the HCL writer's alignment runs (`start = end` with mutated arithmetic). All three were rewritten bounded (slice patterns that consume at least one character a step, a range up to the next `/`, `split_inclusive`), so no mutant can make them endless.
- After, on the reorganized crate: 570 mutants, 493 caught, 18 missed, 3 timeouts, 56 unviable. Each survivor was then killed by a behaviour-named test (template conversions, one bridge per VLAN, a VM's trunk and link impairment steps) or went with its code (a redundant separator test in `Module::render`; a machine's `before` scripts, removed from the plan as the spec's design says). The 3 timeouts were caught mutants whose suite ran past the limit on a loaded host (a test spawning `sh`), and were caught again on rerun.
- The run manifest pieces (`run_manifest`, added by the run-manifest work) were tested only through Compose; render's own tests now cover each.
- Closure: every survivor and new mutant rerun, 83 tested, 83 caught. Nothing is listed in `.cargo/mutants.toml`.

## Fuzzing
- `cargo xtask fuzz --time 300 quoting`: 782,018 runs in 301 s, no crash. The target now checks the HCL builder's own strings (what Terraform modules contain), not the removed `hcl()` helper, beside Ruby, POSIX sh and PowerShell literals; it shares `tests/quoting/literals.rs` with the quoting properties.
