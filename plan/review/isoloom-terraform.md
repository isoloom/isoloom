# Review: isoloom-terraform (Phase 1c)

The Terraform backends (`proxmox`, `esxi`, `cloud-vm` on six clouds, `cloud-services`, and the
Terraform halves of `cloud-docker` and `docker-vm`). One line per 3b principle and 3d item: `ok`,
or the finding and its fix commit.

## 3b design rules
- Functional core, imperative shell: ok (every module is a pure function of a planned `Environment`; no I/O, clock or environment variable).
- Ports and adapters: ok (implements the `Backend` port of `isoloom-render`; Terraform is never run).
- One-way dependencies: ok (spec, plan and render only; no other backend; the containers' host size moved to `isoloom_render::container::HostSize`, which Vagrant uses too).
- Parse, don't validate: ok (a cloud's rules are typed: `Rules` with `Subnets`, `Kept`, `Images`, `Volumes`, `Mtu`, `Networks`; a gateway route is a `Hop`; an LXC container is a `Container` of name, plan and spec).
- Typed errors: ok (`Error` in `error.rs`, `#[source]` on the builder's error, mapped once into `RenderError`).
- No hidden state: ok.
- Determinism: ok (ordered maps throughout; `rendering_twice_gives_the_same_files` for every target).
- One way to do each thing: ok (a cloud module's head is `cloud::head`, its frame `cloud::vm::Driver` and `cloud::docker::Driver`; a server module's frame is `server::Parts` and its head `server::head`; check hosts through `isoloom_render::checks::address_of`; SSH connections through one `provisioner::ssh`).

## 3b principles
- Names, one vocabulary, no stutter: ok (`CloudVmModule`, `CloudVmBackend`, `ProxmoxBackend` and `EsxiBackend` keep their names beside the `Backend` trait and the registry's other backends).
- Small functions: ok (all under 60 lines, complexity under 15).
- Few arguments, no flag arguments: ok.
- Single responsibility, small modules: ok (by concept: `module`, `machine`, `cloud` with one folder per cloud and its `vm`/`docker` drivers, `server` with `proxmox` and `esxi`; each cloud's private DNS is in its own folder; the largest file has 337 lines).
- Open/closed, Liskov, interface segregation, dependency inversion: ok (a cloud is a driver of fn pointers the shared frame calls; adding one touches its folder and one `match`).
- DRY: ok (no `dup/clone`: the six drivers keep only what differs; rules, routes through gateways, `reach` rules, outputs, first addresses, the module heads and the server frame are each written once).
- KISS and YAGNI: ok (the public API is the backends, `Cloud`, the two `cloud-docker` functions, `docker_vm_module` and the error).
- Command-query separation, no hidden side effects: ok (builders return values; a driver writes only into the module it is given).
- Law of Demeter: ok (`reach::own`, `reach::opening`, `gateways::through` and `toward` replace chains of `env.spec().machines[name]`).
- Errors as types, no null: ok (no `unwrap`, `expect`, `panic` or indexing outside tests; a machine or network the plan guarantees is looked up with `get`).
- Clean boundaries with third-party code: ok (HCL through `isoloom-render`'s builder only).
- Clean tests (F.I.R.S.T., one concept each): ok (unit tests next to the code with `crate::testing`, integration tests in `tests/` sharing `tests/testing` and returning a `Result` instead of allowing `unwrap`).
- Comments say why: ok.
- Formatting: ok (rustfmt).
- No dead code: ok (`unused` at deny; machete clean).

## 3d Rust practices
- Workspace hygiene: ok (proptest from the workspace).
- Common traits: ok (`Cloud`: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`, `Display`; the backends: `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, `Eq`, `Hash`; `Error`: `Debug`).
- Conversions through `From`: ok (`From<Cloud> for CloudVmModule`, `From<Error> for RenderError`).
- Types carry meaning, `#[non_exhaustive]`: ok (`Cloud` and `Error` are non-exhaustive).
- Ownership: ok (borrows the plan, returns owned files).
- `#[must_use]`: ok (every public function returning a value).
- Visibility: ok (`pub(crate)` everywhere but the API; no `unreachable_pub`).
- Library errors with `thiserror`, `#[source]` and a `Result` alias: ok.
- Error messages: ok (lowercase, no trailing period; every refusal's text unchanged).
- No panics in library code: ok.
- Sync or async, observability: ok (synchronous and pure; it logs nothing).
- Crate docs with a runnable example, `# Errors` and `# Panics`: ok (the crate example renders a Proxmox lab).
- Docs build clean: ok.
- Behaviour-named tests, one concept each: ok.
- Property tests for parsers and quoting: ok (`module::ident`, Proxmox's PCI addresses and its OCI image references).
- Snapshots only for whole files: ok (whole modules; byte-identical through the reorganization, with the CLI's 336 example snapshots).

## 1c organization
- `lib.rs` holds only the module tree and the re-exported API: ok.
- Modules by domain concept: ok.
- One `Error` in `error.rs` with a `Result` alias: ok.
- Tests next to the code, integration tests in `tests/`, shared helpers in `testing`: ok.

## Left for Phase 3 (behaviour, unchanged here)
- The Proxmox SDN destroy order, the node default, the router timeout, and the `terraform_data` guest SSH.
