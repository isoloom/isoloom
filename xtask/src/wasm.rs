//! `wasm`: `isoloom-wasm`, the topology editor's engine, built for the browser, which downloads
//! it. `wasm-pack` builds `crates/frontends/isoloom-wasm` into `web/pkg/` with Cargo's release
//! profile set for size (opt-level `z`, LTO, one codegen unit: Isoloom's targets bring every
//! backend, and a download's zip needs them all), only for this build. With `--copy-to <dir>`,
//! the two files a page serves (the JavaScript glue and the WebAssembly) are copied into that
//! folder too.

use std::path::PathBuf;
use std::process::Command;

use crate::{Res, process, repo};

/// The crate built, from the repository root.
const CRATE: &str = "crates/frontends/isoloom-wasm";

/// Where wasm-pack writes the package, from the crate's folder (`web/pkg/` at the root).
const OUT_DIR: &str = "../../../web/pkg";

/// The files a page serves.
const SERVED: [&str; 2] = ["isoloom_wasm.js", "isoloom_wasm_bg.wasm"];

/// The arguments of `wasm`.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// A folder that gets the files a page serves (the JavaScript glue and the WebAssembly).
    #[arg(long, value_name = "DIR")]
    copy_to: Option<PathBuf>,
}

/// Builds the engine into `web/pkg/`, then copies what a page serves into `--copy-to`'s folder.
pub(crate) fn build(a: &Args) -> Res<()> {
    let root = repo::root();
    let mut cmd = Command::new("wasm-pack");
    cmd.args(["build", CRATE, "--target", "web", "--out-dir", OUT_DIR, "--release"])
        .current_dir(&root)
        .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "z")
        .env("CARGO_PROFILE_RELEASE_LTO", "true")
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");
    process::run(&mut cmd)?;
    let Some(dir) = &a.copy_to else { return Ok(()) };
    for file in SERVED {
        std::fs::copy(root.join("web/pkg").join(file), dir.join(file)).map_err(|e| format!("copying {file} into {}: {e}", dir.display()))?;
    }
    Ok(())
}
