use std::path::{Path, PathBuf};

use isoloom_spec::model::SecretSource;

use super::*;

fn spec(secrets: &str) -> Spec {
    let yaml = format!("version: 1\nname: s\nsecrets:\n{secrets}machines:\n  a: {{ docker: {{ image: alpine }} }}\n");
    isoloom_spec::parse(&yaml).unwrap()
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("isoloom-secrets-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn vars(name: &str) -> Option<String> {
    match name {
        "DB_PASSWORD" => Some("from-env".into()),
        "HOME" => Some("/nonexistent-home".into()),
        _ => None,
    }
}

fn lookup(project: &Path) -> Lookup<'_> {
    Lookup { project, var: &vars }
}

#[test]
fn reads_the_environment_and_files_relative_to_the_project() {
    let d = dir("read");
    std::fs::write(d.join("key.txt"), "from-file").unwrap();
    let s = spec("  db: { env: DB_PASSWORD }\n  key: { file: key.txt }\n");
    let got = resolve(&s, &lookup(&d)).unwrap();
    assert_eq!(got[0].0, "db");
    assert_eq!(got[0].1.expose(), "from-env");
    assert_eq!(got[1].1.expose(), "from-file");
    // Never printed, even by Debug.
    assert!(!format!("{got:?}").contains("from-env"));
}

#[test]
fn every_missing_secret_is_reported_at_once_without_a_value() {
    let d = dir("missing");
    let s = spec("  a: { env: NOT_SET }\n  b: { file: nope.txt }\n  c: { env: DB_PASSWORD }\n");
    let errors = resolve(&s, &lookup(&d)).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].name, "a");
    assert!(errors[0].to_string().contains("`NOT_SET` isn't set"), "{}", errors[0]);
    assert!(errors[1].to_string().contains("nope.txt can't be read"), "{}", errors[1]);
    assert!(!errors.iter().any(|e| e.to_string().contains("from-env")));
}

#[test]
fn a_home_path_reads_from_home() {
    let at = lookup(Path::new("/project"));
    assert_eq!(file_path("~/s/key", &at), PathBuf::from("/nonexistent-home/s/key"));
    assert_eq!(file_path("/abs/key", &at), PathBuf::from("/abs/key"));
    assert_eq!(file_path("rel/key", &at), PathBuf::from("/project/rel/key"));
}

/// A stand-in for a store's command: prints its arguments, or fails.
#[cfg(unix)]
fn fake(d: &Path, name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = d.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[cfg(unix)]
#[test]
fn vault_and_sops_run_their_commands() {
    let d = dir("cmds");
    let vault = fake(&d, "vault", "printf '%s ' \"$@\"");
    let sops = fake(&d, "sops", "printf '%s|' \"$@\"; echo");
    let s = spec("  v: { vault: 'secret/app#db_password' }\n  o: { sops: 'enc.yaml#db.password' }\n");
    let store_of = |src: &SecretSource| -> Box<dyn SecretStore> {
        match src {
            SecretSource::Vault { .. } => Box::new(VaultStore { program: vault.clone() }),
            _ => Box::new(SopsStore { program: sops.clone() }),
        }
    };
    let got = resolve_with(&s, &lookup(&d), &store_of).unwrap();
    assert_eq!(got[0].1.expose(), "kv get -field=db_password secret/app ");
    let enc = d.join("enc.yaml");
    assert_eq!(got[1].1.expose(), format!("--decrypt|--extract|[\"db\"][\"password\"]|{}|", enc.display()));
}

#[cfg(unix)]
#[test]
fn a_failing_command_says_why_and_a_missing_one_says_so() {
    let d = dir("fail");
    let vault = fake(&d, "vault", "echo 'permission denied' >&2; exit 2");
    let s = spec("  v: { vault: 'secret/app#x' }\n");
    let failing = |_: &SecretSource| -> Box<dyn SecretStore> { Box::new(VaultStore { program: vault.clone() }) };
    let e = resolve_with(&s, &lookup(&d), &failing).unwrap_err();
    assert_eq!(e[0].why, "`vault` failed: permission denied");
    let missing = |_: &SecretSource| -> Box<dyn SecretStore> {
        Box::new(VaultStore {
            program: "/nonexistent/vault".into(),
        })
    };
    let e = resolve_with(&s, &lookup(&d), &missing).unwrap_err();
    assert_eq!(e[0].why, "`vault` isn't installed");
}

#[test]
fn a_value_with_a_nul_is_refused() {
    let d = dir("nul");
    std::fs::write(d.join("bin"), b"a\0b").unwrap();
    let s = spec("  b: { file: bin }\n");
    let e = resolve(&s, &lookup(&d)).unwrap_err();
    assert!(e[0].why.contains("NUL"), "{e:?}");
}
