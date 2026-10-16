//! A machine's service resolved (design §4.9): the release its version picks, the OS checked,
//! `when:` and `for_each:` applied, placeholders and pins filled in, the official image only on
//! its own OS.

use isoloom_spec::syntax::{Code, Step};

use crate::testing::{demo, resolve};

/// The kinds of `steps`, in order.
fn kinds(steps: &[Step]) -> Vec<&'static str> {
    steps.iter().map(Step::kind).collect()
}

#[test]
fn a_service_without_a_version_is_its_default_release() {
    let r = resolve(&demo().unwrap(), "debian@12", "demo", "{}").unwrap();
    assert_eq!((r.entry.as_str(), r.release.as_str()), ("demo", "2.4.1"));
    let port = |name: &str| r.ports.iter().find(|(n, _)| n.as_str() == name).map(|(_, p)| p.port.get());
    assert_eq!((port("web"), port("admin")), (Some(8080), Some(9090)));
    assert_eq!(r.data.get("state").map(ToString::to_string).as_deref(), Some("/var/lib/demo"));
}

#[test]
fn a_version_prefix_picks_the_newest_release_and_an_exact_one_itself() {
    assert_eq!(resolve(&demo().unwrap(), "debian@12", "demo@2", "{}").unwrap().release.as_str(), "2.5.0");
    assert_eq!(resolve(&demo().unwrap(), "debian@12", "demo@2.4.1", "{}").unwrap().release.as_str(), "2.4.1");
    assert_eq!(
        resolve(&demo().unwrap(), "ubuntu@24.04", "demo@2.4", "{}").unwrap().release.as_str(),
        "2.4",
        "an exact release wins over 2.4.1"
    );
}

#[test]
fn an_unknown_entry_is_an_unknown_reference_with_the_closest_name() {
    let p = resolve(&demo().unwrap(), "debian@12", "dmeo", "{}").unwrap_err();
    assert_eq!((p[0].code, p[0].path.as_str()), (Code::UnknownReference, "machines.web.service"));
    assert_eq!(p[0].reason, "the catalogue has no entry `dmeo`");
    assert_eq!(p[0].fix.as_deref(), Some("did you mean `demo`?"));
}

#[test]
fn a_version_no_release_has_lists_the_releases() {
    let p = resolve(&demo().unwrap(), "debian@12", "demo@3", "{}").unwrap_err();
    assert_eq!((p[0].code, p[0].path.as_str()), (Code::InvalidValue, "machines.web.service"));
    assert_eq!(p[0].reason, "demo has no release `3`");
    assert_eq!(p[0].fix.as_deref(), Some("one of its releases: 2.4.1 (stable), 2.5.0 (edge), 2.4 (os)"));
}

#[test]
fn an_os_the_release_doesnt_run_on_lists_its_oses() {
    let p = resolve(&demo().unwrap(), "alpine@3.24", "demo", "{}").unwrap_err();
    assert_eq!((p[0].code, p[0].path.as_str()), (Code::InvalidValue, "machines.web.os"));
    assert_eq!(p[0].reason, "demo 2.4.1 doesn't run on alpine@3.24");
    assert_eq!(p[0].fix.as_deref(), Some("one of its OSes: debian@12, debian@13, rocky@9"));
}

#[test]
fn when_picks_the_steps_of_the_family_the_release_and_the_settings() {
    let debian = resolve(&demo().unwrap(), "debian@12", "demo", "{}").unwrap();
    assert_eq!(kinds(&debian.install), ["repository", "packages"]);
    assert_eq!(kinds(&debian.configure), ["template", "service"], "no greeting, no users, no peer");
    let Step::Repository(repo) = &debian.install[0] else { panic!() };
    assert_eq!(repo.url.as_str(), "https://example.org/stable/debian");
    assert_eq!(repo.key.sha256.as_str(), "2".repeat(64), "the key's fingerprint becomes lock.yml's SHA-256");
    let Step::Packages(p) = &debian.install[1] else { panic!() };
    assert_eq!(
        p[0].version.as_ref().map(ToString::to_string).as_deref(),
        Some("2.4.1-1~bookworm"),
        "lock.yml's exact version"
    );

    let rocky = resolve(&demo().unwrap(), "rocky@9", "demo", "{}").unwrap();
    let Step::Repository(rpm) = &rocky.install[0] else { panic!() };
    assert_eq!(
        (rpm.url.as_str(), rpm.key.sha256.as_str()),
        ("https://example.org/stable/rpm/9", "1".repeat(64).as_str())
    );

    let ubuntu = resolve(&demo().unwrap(), "ubuntu@24.04", "demo@2.4", "{}").unwrap();
    assert_eq!(kinds(&ubuntu.install), ["packages"], "a distro release adds no repository");
    let Step::Packages(distro) = &ubuntu.install[0] else { panic!() };
    assert_eq!(distro[0].version, None, "a distro release follows its OS's archive");

    let greeted = resolve(&demo().unwrap(), "debian@12", "demo", "{ greeting: hello there }").unwrap();
    assert_eq!(kinds(&greeted.configure), ["template", "command", "service"]);
    let Step::Command(c) = &greeted.configure[1] else { panic!() };
    assert!(format!("{c:?}").contains("echo hello there > /etc/demo/greeting"), "{c:?}");
}

#[test]
fn for_each_repeats_a_step_for_each_item_with_its_fields() {
    let r = resolve(&demo().unwrap(), "debian@12", "demo", "{ users: [{ name: ann }, { name: bob }] }").unwrap();
    assert_eq!(kinds(&r.configure), ["template", "template", "template", "service"]);
    let to: Vec<String> = r
        .configure
        .iter()
        .filter_map(|s| if let Step::Template(placed) = s { Some(placed.to.to_string()) } else { None })
        .collect();
    assert_eq!(to, ["/etc/demo/demo.conf", "/etc/demo/users/ann.conf", "/etc/demo/users/bob.conf"]);
    let bob = r
        .files
        .iter()
        .find(|(k, _)| k.as_str() == ".isoloom/catalogue/demo/item-2/files/user.conf")
        .map(|(_, t)| t.as_str());
    assert_eq!(bob, Some("# SPDX-License-Identifier: Apache-2.0\nuser bob\n"));
}

#[test]
fn a_template_fills_the_entry_placeholders_and_keeps_the_lab_ones() {
    let r = resolve(&demo().unwrap(), "debian@13", "demo", "{ port: 8443, peer: 'app:7000' }").unwrap();
    let Step::Template(t) = &r.configure[0] else { panic!() };
    assert_eq!(t.from.as_str(), ".isoloom/catalogue/demo/files/demo.conf", "the step names the rendered text");
    let text = r.files.get(&t.from).map(String::as_str);
    assert_eq!(
        text,
        Some("# SPDX-License-Identifier: Apache-2.0\nlisten 8443\nrelease 2.4.1 on debian 13\nhost {{ machine.name }}\n")
    );
    let peer = r
        .files
        .iter()
        .find(|(k, _)| k.as_str().ends_with("peer.conf"))
        .map(|(_, peer_text)| peer_text.as_str());
    assert_eq!(
        peer,
        Some("# SPDX-License-Identifier: Apache-2.0\npeer {{ machines.app.addresses.lab }}:7000\n"),
        "an endpoint is the machine's address, per target"
    );
}

#[test]
fn the_official_image_serves_only_its_own_os() {
    let on_13 = resolve(&demo().unwrap(), "debian@13", "demo", "{ port: 8443 }").unwrap();
    let image = on_13.image.expect("debian@13 has the official image");
    assert_eq!(image.reference, format!("docker.io/library/demo:2.4.1@sha256:{}", "3".repeat(64)));
    assert_eq!((image.entrypoint, image.command), (vec!["/entry.sh".to_owned()], vec!["demo".to_owned()]));
    assert_eq!(image.environment.get("DEMO_PORT").map(String::as_str), Some("8443"));
    assert!(
        resolve(&demo().unwrap(), "debian@12", "demo", "{}").unwrap().image.is_none(),
        "debian@12 builds from its own image"
    );
}

#[test]
fn a_password_gets_its_value_file_and_its_owner_a_start_step() {
    let r = resolve(&demo().unwrap(), "debian@12", "demo", "{ admin: { secret: admin } }").unwrap();
    assert_eq!(r.value_files.len(), 1);
    assert_eq!(r.value_files[0].path, "/run/isoloom/with/admin");
    assert_eq!(r.value_files[0].owner.as_ref().map(ToString::to_string).as_deref(), Some("demo"));
    assert_eq!(kinds(&r.start), ["command", "command"], "chown to its owner, then the entry's own start step");
    assert!(
        format!("{:?}", r.start[1]).contains("/run/isoloom/with/admin"),
        "the entry reads the file, never the value"
    );
}

#[test]
fn several_services_each_read_their_own_settings() {
    let catalogue = demo().unwrap();
    let both = crate::testing::lab("debian@12", "[demo, mini]", "{ port: 81, size: 2 }").unwrap();
    let resolved = catalogue.resolve(&both, "web").unwrap();
    assert_eq!(resolved.iter().map(|r| r.entry.as_str()).collect::<Vec<_>>(), ["demo", "mini"]);
    // Each service keeps the `with:` keys that are its own: demo reads `port`, mini reads `size`.
    let web = resolved[0].ports.iter().find(|(n, _)| n.as_str() == "web").map(|(_, p)| p.port.get());
    assert_eq!(web, Some(81), "demo's web port comes from its `port` setting");
    assert!(format!("{:?}", resolved[1].configure).contains("--size 2"), "mini applies its `size`: {:?}", resolved[1].configure);
    let nope = crate::testing::lab("debian@12", "[demo, mini]", "{ port: 81, nope: 1 }").unwrap();
    let p = catalogue.resolve(&nope, "web").unwrap_err();
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(
        (p[0].code, p[0].path.as_str(), p[0].reason.as_str()),
        (Code::UnknownField, "machines.web.with.nope", "no service of the machine has a setting `nope`")
    );
    let typo = crate::testing::lab("debian@12", "[demo, mnii]", "{}").unwrap();
    let unknown = catalogue.resolve(&typo, "web").unwrap_err();
    assert_eq!(unknown[0].path.as_str(), "machines.web.service[1]", "each service at its index");
}

#[test]
fn passwords_in_a_list_get_value_files_each_with_its_own_setting_owner() {
    let r = resolve(
        &demo().unwrap(),
        "debian@12",
        "demo",
        "{ admin: { secret: admin }, users: [{ name: ann, password: { secret: admin } }] }",
    )
    .unwrap();
    let admin = r.value_files.iter().find(|f| f.path.contains("admin")).expect("admin's value file");
    assert_eq!(admin.owner.as_ref().map(ToString::to_string).as_deref(), Some("demo"));
    // The list item's password is collected too, with no owner (the `users` setting has none).
    let listed = r
        .value_files
        .iter()
        .find(|f| !f.path.contains("admin"))
        .expect("the list item's password value file");
    assert!(listed.owner.is_none(), "the `users` setting has no owner of its own");
}

#[test]
fn an_integer_setting_accepts_a_value_at_its_maximum() {
    let mut files = crate::testing::fixture_files().unwrap();
    let mini = files.get("mini/entry.yml").cloned().unwrap();
    files.insert(
        "mini/entry.yml".into(),
        mini.replace("minimum: 1, default: 1", "minimum: 1, maximum: 5, default: 1"),
    );
    let catalogue = crate::testing::load(&files).unwrap();
    let lab = crate::testing::lab("debian@12", "mini", "{ size: 5 }").unwrap();
    assert!(catalogue.resolve(&lab, "web").is_ok(), "5 is within 1..=5");
}

#[test]
fn an_empty_catalogue_says_to_write_an_app() {
    let empty = isoloom_catalogue::Catalogue::from_files([]).unwrap();
    let p = resolve(&empty, "debian@12", "nginx", "{}").unwrap_err();
    assert_eq!(p[0].fix.as_deref(), Some("a custom app: `app: { image: <ref> }`, or `os` and `app.steps`"));
    let listed = resolve(&demo().unwrap(), "debian@12", "postgres", "{}").unwrap_err();
    assert_eq!(listed[0].fix.as_deref(), Some("its entries: `demo`, `mini`, or a custom `app:`"));
}
