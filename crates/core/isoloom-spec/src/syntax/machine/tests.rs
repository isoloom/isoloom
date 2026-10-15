use super::*;

fn machine(yaml: &str) -> Result<Machine, String> {
    serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
}

#[test]
fn a_machine_is_a_service_an_app_or_bare() {
    let m = machine("{ os: debian@12, networks: { lab: 10 } }").unwrap();
    assert_eq!(m.kind, Kind::Bare);
    let m = machine("{ os: debian@12, service: postgres@16, with: { database: books } }").unwrap();
    let Kind::Service(s) = m.kind else { panic!() };
    assert_eq!((s.services[0].as_str(), s.with.len()), ("postgres@16", 1));
    let m = machine("{ app: { build: build/web }, ports: { site: { port: 8000, protocol: http } } }").unwrap();
    assert!(m.app().is_some() && m.os.is_none());
    let m = machine("{ os: debian, service: [nginx, php-fpm] }").unwrap();
    assert!(matches!(m.kind, Kind::Service(ref s) if s.services.len() == 2));
}

#[test]
fn a_service_and_an_app_together_are_refused() {
    let e = machine("{ os: debian@12, service: nginx, app: { image: nginx } }").unwrap_err();
    assert!(e.contains("both given"), "{e}");
    let e = machine("{ os: debian@12, with: { a: 1 } }").unwrap_err();
    assert!(e.contains("goes with `service`"), "{e}");
}

#[test]
fn old_and_unknown_keys_are_not_read() {
    for key in [
        "docker: { image: x }",
        "vm: { os: debian }",
        "inputs: [A]",
        "secrets: [a]",
        "arch: arm64",
        "before: [a.sh]",
    ] {
        let e = machine(&format!("{{ {key} }}")).unwrap_err();
        assert!(e.contains("unknown field"), "{key}: {e}");
    }
}

#[test]
fn settings_read_as_written() {
    let m = machine("{ os: alpine, networks: { lan: [10, 11] }, sysctls: { net.core.somaxconn: 4096 }, hardware: { arch: arm64 }, setup: [a.sh] }").unwrap();
    assert_eq!(m.networks["lan"], Octets::Several(vec![10, 11]));
    assert_eq!(m.sysctls["net.core.somaxconn"], "4096");
    assert_eq!(m.setup, ["a.sh"]);
}
