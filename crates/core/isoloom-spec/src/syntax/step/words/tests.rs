use proptest::prelude::*;

use super::*;

/// Whether `value` matches the newtype's JSON Schema pattern, read the way an editor reads it.
fn schema_accepts<T: JsonSchema>(value: &str) -> bool {
    let schema = serde_json::Value::from(schemars::schema_for!(T));
    jsonschema::is_valid(&schema, &serde_json::json!(value))
}

#[test]
fn each_name_takes_what_its_kind_is_written_as() {
    for ok in ["nginx", "php8.2-fpm", "getty@tty1", "W3SVC"] {
        assert!(ok.parse::<UnitName>().is_ok(), "{ok}");
    }
    for ok in ["www-data", "_apt", "Administrator", "svc.backup"] {
        assert!(ok.parse::<UserName>().is_ok(), "{ok}");
    }
    assert!("Remote Desktop Users".parse::<GroupName>().is_ok() && "ops ".parse::<GroupName>().is_err());
    for ok in ["nginx", "libapache2-mod-php", "Web-Server", "libc6:i386", "g++"] {
        assert!(ok.parse::<PackageName>().is_ok(), "{ok}");
    }
    assert!("pgdg".parse::<RepositoryName>().is_ok() && "Pgdg".parse::<RepositoryName>().is_err());
    for ok in ["1.30.6-1~bookworm", "1:2.4.62-1", "1.30.6-r1", "16"] {
        assert!(ok.parse::<PackageVersion>().is_ok(), "{ok}");
    }
}

#[test]
fn a_name_breaking_its_rule_says_the_rule() {
    let e = "nginx=1.2".parse::<PackageName>().unwrap_err().to_string();
    assert!(e.contains("its version goes in `version`"), "{e}");
    let unit = "rm -rf /".parse::<UnitName>().unwrap_err().to_string();
    assert!(unit.contains("`rm -rf /` isn't a valid unit name"), "{unit}");
}

#[test]
fn paths_are_absolute_in_the_machine_and_relative_in_the_project() {
    for ok in ["/etc/app.toml", "C:\\inetpub\\wwwroot", "c:/tools"] {
        assert!(ok.parse::<AbsPath>().is_ok(), "{ok}");
    }
    for bad in ["etc/app.toml", "", "C:", "/a\nb"] {
        assert!(bad.parse::<AbsPath>().is_err(), "{bad:?}");
    }
    for ok in ["provision/web.sh", "site/", "a..b/c", "./x.sh"] {
        assert!(ok.parse::<ProjectPath>().is_ok(), "{ok}");
    }
    for bad in ["/etc/passwd", "../secret", "site/../../x", "C:\\x", "\\x", ""] {
        assert!(bad.parse::<ProjectPath>().is_err(), "{bad:?}");
    }
}

#[test]
fn every_fetch_is_https_and_every_checksum_a_sha256() {
    assert!("https://nginx.org/keys/nginx_signing.key".parse::<HttpsUrl>().is_ok());
    for bad in ["http://nginx.org/", "https://", "https:///x", "https://a b/"] {
        assert!(bad.parse::<HttpsUrl>().is_err(), "{bad:?}");
    }
    assert!("ab".repeat(32).parse::<Sha256>().is_ok());
    assert!("AB".repeat(32).parse::<Sha256>().is_err() && "ab".repeat(31).parse::<Sha256>().is_err());
}

#[test]
fn a_decimal_version_is_quoted_not_read_as_a_number() {
    let e = serde_yaml_ng::from_str::<PackageVersion>("2.40").unwrap_err().to_string();
    assert!(e.contains("quote the version"), "{e}");
    assert_eq!(serde_yaml_ng::from_str::<PackageVersion>("16").unwrap().as_str(), "16");
}

proptest! {
    /// The type and its schema agree: an editor accepts exactly what the parser does.
    #[test]
    fn the_schema_pattern_is_the_parse_rule(value in "[ -~]{0,12}|/[a-z./ -]{0,8}|[a-z]{1,4}:[\\\\/][a-z]{0,4}") {
        prop_assert_eq!(value.parse::<UnitName>().is_ok(), schema_accepts::<UnitName>(&value));
        prop_assert_eq!(value.parse::<UserName>().is_ok(), schema_accepts::<UserName>(&value));
        prop_assert_eq!(value.parse::<GroupName>().is_ok(), schema_accepts::<GroupName>(&value));
        prop_assert_eq!(value.parse::<PackageName>().is_ok(), schema_accepts::<PackageName>(&value));
        prop_assert_eq!(value.parse::<RepositoryName>().is_ok(), schema_accepts::<RepositoryName>(&value));
        prop_assert_eq!(value.parse::<AbsPath>().is_ok(), schema_accepts::<AbsPath>(&value));
        prop_assert_eq!(value.parse::<ProjectPath>().is_ok(), schema_accepts::<ProjectPath>(&value));
        prop_assert_eq!(value.parse::<HttpsUrl>().is_ok(), schema_accepts::<HttpsUrl>(&value));
        prop_assert_eq!(value.parse::<Sha256>().is_ok(), schema_accepts::<Sha256>(&value));
    }

    /// A value that parses reads back as itself, and holds nothing a shell would read as syntax.
    #[test]
    fn a_name_reads_back_as_itself(value in "\\PC{0,16}") {
        if let Ok(unit) = value.parse::<UnitName>() {
            prop_assert_eq!(unit.to_string().parse::<UnitName>().ok(), Some(unit.clone()));
            prop_assert!(!unit.as_str().contains(['$', '`', '\'', '"', ';', ' ', '|', '&']));
        }
        if let Ok(package) = value.parse::<PackageName>() {
            prop_assert!(!package.as_str().contains(['$', '`', '\'', '"', ';', ' ', '=', '|', '&']));
        }
        if let Ok(path) = value.parse::<ProjectPath>() {
            prop_assert!(!path.as_str().split(['/', '\\']).any(|p| p == ".."));
        }
    }
}
