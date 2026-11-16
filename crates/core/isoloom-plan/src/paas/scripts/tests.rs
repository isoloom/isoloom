//! The scripts run with `sh` on secrets made to break them, each output read back as its own
//! reader reads it: JSON by a JSON parser, `.properties` as `java.util.Properties` loads it, the
//! JAAS line as Kafka's `JaasConfig` tokenizes it, `--add-scram` as Kafka's `ScramParser` splits it.

#![expect(clippy::disallowed_methods, reason = "the tests run the scripts they check with sh: test code, not an adapter")]

use std::path::Path;
use std::process::Command;

use super::*;

/// Secrets with every character the formats treat specially.
const NASTY: [&str; 4] = ["pa\"ss", "back\\slash", "both\\\"mix\\\\ed\"", "tab\there$HOME`id`'q'"];

/// Runs `script` with `sh -eu` in `dir`; its status, and what it said on stderr.
fn run(script: &str, dir: &Path) -> (bool, String) {
    let out = Command::new("sh").arg("-euc").arg(script).current_dir(dir).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A folder holding the secret files `files`.
fn secrets(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("in")).unwrap();
    for (name, value) in files {
        std::fs::write(dir.path().join("in").join(name), value).unwrap();
    }
    dir
}

/// A `.properties` file's value of `key`, as `java.util.Properties::load` reads it (`\x` is `x`,
/// with `\t`, `\n`, `\r`, `\f` and `\uXXXX` the escapes Java knows).
fn property(text: &str, key: &str) -> String {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no {key} in {text}"));
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{c}'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The quoted value of option `name` in a JAAS line, as Java's `StreamTokenizer` reads a quoted
/// string (`\` escapes the next character; `\t`, `\n` and the like are those characters).
fn jaas_option(line: &str, name: &str) -> String {
    let (_, quoted) = line.split_once(&format!("{name}=\"")).unwrap_or_else(|| panic!("no {name} in {line}"));
    let mut out = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => break,
            },
            other => out.push(other),
        }
    }
    panic!("unterminated {name} in {line}")
}

#[test]
fn seaweedfs_takes_any_key_as_json_reads_it() {
    for secret in NASTY {
        let dir = secrets(&[("access-key", secret), ("secret-key", &format!("s-{secret}"))]);
        let (ok, err) = run(&seaweedfs_config("in", "out/s3.json"), dir.path());
        assert!(ok, "{secret:?}: {err}");
        let text = std::fs::read_to_string(dir.path().join("out/s3.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{secret:?}: {e}: {text}"));
        let creds = &json["identities"][0]["credentials"][0];
        assert_eq!(creds["accessKey"], secret, "{text}");
        assert_eq!(creds["secretKey"], format!("s-{secret}"), "{text}");
    }
}

#[test]
#[ignore = "needs openssl 3: the Kafka script salts the password with `openssl kdf`; `cargo xtask test` runs it"]
fn kafka_takes_any_password_in_both_properties_files_and_salts_it_for_add_scram() {
    for secret in NASTY {
        let dir = secrets(&[("user", "shop"), ("password", secret)]);
        let (ok, err) = run(&kafka_config("in", "bus.shop.internal", "state"), dir.path());
        assert!(ok, "{secret:?}: {err}");
        for (file, key) in [
            ("server.properties", "listener.name.sasl_plaintext.scram-sha-512.sasl.jaas.config"),
            ("client.properties", "sasl.jaas.config"),
        ] {
            let text = std::fs::read_to_string(dir.path().join("state").join(file)).unwrap();
            let jaas = property(&text, key);
            assert_eq!(jaas_option(&jaas, "username"), "shop", "{file}: {jaas}");
            assert_eq!(jaas_option(&jaas, "password"), secret, "{file}: {jaas}");
        }
        // As ScramParser reads it: the mechanism, `[...]`, components split on `,`, each at its
        // first `=`. The password isn't there, only its salted form.
        let arg = std::fs::read_to_string(dir.path().join("state/scram.arg")).unwrap();
        let inner = arg
            .strip_prefix("SCRAM-SHA-512=[")
            .and_then(|a| a.strip_suffix(']'))
            .unwrap_or_else(|| panic!("{arg}"));
        let parts: std::collections::BTreeMap<&str, &str> = inner.split(',').map(|c| c.split_once('=').unwrap()).collect();
        assert_eq!(
            parts.keys().copied().collect::<Vec<_>>(),
            ["iterations", "name", "salt", "saltedpassword"],
            "{arg}"
        );
        assert_eq!(parts["name"], "shop");
        assert_eq!(parts["iterations"], SCRAM_ITERATIONS.to_string());
        assert!(!arg.contains(secret), "the password itself never goes in --add-scram");
        assert_eq!(parts["saltedpassword"].len(), 88, "64 bytes, base64: {arg}");
    }
}

#[test]
#[ignore = "needs openssl 3 and python3: python's PBKDF2 checks the script's `openssl kdf`; `cargo xtask test` runs it"]
fn the_salted_password_is_pbkdf2_hmac_sha512_of_the_password() {
    // The same derivation by a second implementation.
    let secret = NASTY[2];
    let dir = secrets(&[("user", "shop"), ("password", secret)]);
    assert!(run(&kafka_config("in", "bus.shop.internal", "state"), dir.path()).0);
    let arg = std::fs::read_to_string(dir.path().join("state/scram.arg")).unwrap();
    let field = |k: &str| arg.split(&format!("{k}=")).nth(1).unwrap().split([',', ']']).next().unwrap().to_owned();
    let check = format!(
        "import base64,hashlib,sys; print(base64.b64encode(hashlib.pbkdf2_hmac('sha512', sys.argv[1].encode(), base64.b64decode(sys.argv[2]), {SCRAM_ITERATIONS})).decode())"
    );
    let out = Command::new("python3").args(["-c", &check, secret, &field("salt")]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), field("saltedpassword"));
}

#[test]
fn a_line_break_in_a_secret_or_a_comma_in_the_kafka_user_is_refused_with_a_message() {
    let kafka = |user: &str, password: &str| {
        let dir = secrets(&[("user", user), ("password", password)]);
        run(&kafka_config("in", "h", "state"), dir.path())
    };
    let seaweedfs = |key: &str| {
        let dir = secrets(&[("access-key", key), ("secret-key", "x")]);
        run(&seaweedfs_config("in", "out/s3.json"), dir.path())
    };
    for (case, (ok, err), says) in [
        (
            "a password's line break",
            kafka("shop", "two\nlines"),
            "the Kafka login holds a line break, which its configuration cannot carry",
        ),
        (
            "a user's comma",
            kafka("a,b", "x"),
            "the Kafka user holds `,` or `]`, or is quoted, which --add-scram cannot carry",
        ),
        (
            "an access key's line break",
            seaweedfs("a\nb"),
            "an S3 key holds a line break, which its configuration cannot carry",
        ),
    ] {
        assert_eq!((ok, err.trim_end()), (false, says), "{case}");
    }
}

#[test]
fn the_wait_helper_waits_for_a_file_then_times_out() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wait-for"), wait_helper()).unwrap();
    let (appeared, _) = run("(sleep 1; echo v > fact) & sh ./wait-for fact 10", dir.path());
    assert!(appeared, "the file appeared within the timeout");
    let (never, err) = run("sh ./wait-for never 1", dir.path());
    assert_eq!((never, err.trim_end()), (false, "wait-for: never didn't appear in 1 s"));
}
