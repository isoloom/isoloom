# isoloom-wasm

[`isoloom-topology`](https://crates.io/crates/isoloom-topology) and Isoloom's targets compiled
to WebAssembly: a topology (JSON) becomes an `isoloom.yml`, the files every Isoloom target
generates from it (Compose, Vagrant, Terraform and the rest), and a zip to download. Everything
runs where the WebAssembly runs: in a browser, or in a server's JavaScript runtime.

## Install

Build the JavaScript package with [wasm-pack](https://github.com/wasm-bindgen/wasm-pack) from a
checkout of the repository:

```sh
wasm-pack build crates/frontends/isoloom-wasm --target web --out-dir ../../../web/pkg --release
```

`cargo run -p xtask -- wasm --copy-to <dir>` builds it for size and copies the two files a page
serves (`isoloom_wasm.js`, `isoloom_wasm_bg.wasm`) into `<dir>`. As a Rust dependency
(`cargo add isoloom-wasm`), it is for a crate built for `wasm32-unknown-unknown`: its exports'
errors are JavaScript `Error`s, which exist only in WebAssembly, so built for another target an
export that fails panics.

## Example

The exports JavaScript calls are `plan_json`, `bundle`, `bundle_target`, `own_files_json` and
`choices_json`:

```js
import init, { plan_json, bundle } from "./isoloom_wasm.js";

await init();
const topology = JSON.stringify({
  name: "pair",
  networks: { office: { cidr: "10.10.1.0/24" } },
  nodes: {
    web: { kind: { container: { image: "nginx:1.27-alpine" } }, networks: { office: null } },
    user: { kind: { vm: { os: "debian@12" } }, access: true, networks: { office: null } },
  },
});
const plan = JSON.parse(plan_json(topology)); // { ok, problems, files, addresses, ... }
const zip = bundle(topology);                  // Uint8Array: the project as a zip
```

`plan_json` reports a topology's problems in its plan (`ok`, `error`, `problems`); the other exports throw
a JavaScript `Error` saying why when they can't do their work: `own_files_json`, `bundle` and
`bundle_target` for a topology that is malformed or doesn't compile; `bundle` and `bundle_target`
also for a spec Isoloom finds problems in, or a zip that can't be written; `bundle` for a
project no target generates any file for; and `bundle_target` for a target the project
generates no files for.

## Feature flags

None.

## Documentation

- API: <https://docs.rs/isoloom-wasm>
- Guides and reference: <https://github.com/isoloom/isoloom/tree/main/docs>

Minimum Rust: 1.89. Licence: Apache-2.0.
