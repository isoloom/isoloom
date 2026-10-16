//! The cloud-init carrier: the script delivered compressed in `write_files` and run from
//! `runcmd`, and the user-data size limits.

use isoloom_plan::configure::Resource;
use isoloom_spec::model::Target;

use super::*;
use crate::configure::Configured;
use crate::testing;

/// The resources of machine `m` (as `testing::machine` writes it).
fn resources(env: &isoloom_plan::Environment) -> Vec<Resource> {
    Configured::of(env, "m").expect("a recipe").recipe().resources().cloned().collect()
}

#[test]
fn the_script_is_the_shell_renderers_text_compressed() {
    let env = testing::machine(
        "os: debian@12, app: { steps: [{ packages: [nginx] }, { service: nginx }] }",
        Target::Proxmox,
        &[],
    );
    let c = Configured::of(&env, "m").expect("a recipe");
    let res = resources(&env);
    let carried = configure(&c, &res, true, 0);
    assert_eq!(carried.file.path, "/var/lib/isoloom/configure/configure-0.sh");
    assert_eq!(carried.file.permissions, "0700");
    assert_eq!(carried.command, "sh /var/lib/isoloom/configure/configure-0.sh");
    // The carried content is the shell renderer's own script, only compressed.
    assert_eq!(carried.file.content, gz_b64(&shell::script(&c, &res, true)));
}

#[test]
fn the_run_index_names_the_script() {
    let env = testing::machine("os: debian@12, app: { steps: [{ packages: [curl] }] }", Target::Proxmox, &[]);
    let c = Configured::of(&env, "m").expect("a recipe");
    let res = resources(&env);
    let second = configure(&c, &res, false, 1);
    assert_eq!(second.file.path, "/var/lib/isoloom/configure/configure-1.sh");
    assert_eq!(second.command, "sh /var/lib/isoloom/configure/configure-1.sh");
    // `fresh` false carries a different script (it keeps the change mark) from `fresh` true.
    assert_ne!(second.file.content, configure(&c, &res, true, 1).file.content);
}

#[test]
fn gzip_is_deterministic_and_is_standard_base64() {
    let script = "#!/bin/sh\nset -eu\nisoloom_packages 'nginx'\n";
    let once = gz_b64(script);
    assert_eq!(once, gz_b64(script), "the same script gives the same bytes");
    // Standard base64 (what cloud-init decodes for `gz+b64`): its alphabet, padded.
    assert!(
        once.trim_end_matches('=').chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/'),
        "{once}"
    );
    assert_eq!(once.len() % 4, 0, "{once}");
}

#[test]
fn a_limit_refuses_a_document_over_its_cap_naming_the_overflow() {
    let small = "x".repeat(100);
    assert_eq!(Limit::Aws.fits(&small), Ok(()));
    let big = "x".repeat(Limit::Aws.bytes() + 50);
    let err = Limit::Aws.fits(&big).unwrap_err();
    assert_eq!(err.provider, "AWS");
    assert_eq!(err.cap, 16 * 1024);
    assert_eq!(err.size, 16 * 1024 + 50);
    assert!(err.to_string().contains("over AWS's 16384-byte limit"), "{err}");
    assert_eq!((Limit::Azure.bytes(), Limit::Gcp.bytes()), (64 * 1024, 256 * 1024));
    assert_eq!((Limit::Azure.provider(), Limit::Gcp.provider()), ("Azure", "Google Cloud"));
}
