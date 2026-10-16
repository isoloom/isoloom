//! A broken entry fails to load, with every rule it breaks at its file and field: the
//! catalogue's tests catch it, never a user's spec.

use std::collections::BTreeMap;

use isoloom_catalogue::Error;

use crate::testing::{fixture_files, load};

/// The fixture catalogue with `file` (`demo/entry.yml`) changed by `edit`.
fn edited(file: &str, edit: impl FnOnce(&str) -> String) -> BTreeMap<String, String> {
    edited_in(fixture_files().unwrap_or_default(), file, edit)
}

/// Every finding of `files`, one per line; a failure to load that isn't a broken rule, as is.
fn findings(files: &BTreeMap<String, String>) -> String {
    match load(files) {
        Err(Error::Invalid(found)) => found.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"),
        Err(other) => format!("not a broken rule: {other}"),
        Ok(_) => String::new(),
    }
}

/// `files` break the rule `expected` says.
fn breaks(files: &BTreeMap<String, String>, expected: &str) {
    let found = findings(files);
    assert!(found.contains(expected), "expected `{expected}` in:\n{found}");
}

#[test]
fn the_fixture_breaks_no_rule() {
    assert_eq!(findings(&fixture_files().unwrap()), "");
}

#[test]
fn the_default_is_one_of_the_releases() {
    breaks(
        &edited("demo/entry.yml", |t| t.replace("default: 2.4.1", "default: 9.9")),
        "catalogue/demo/entry.yml: default: `9.9` isn't one of the releases",
    );
}

#[test]
fn a_setting_nothing_uses_would_do_nothing() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("settings:\n", "settings:\n  unused: { type: boolean, description: Nothing. }\n")
    });
    breaks(&files, "settings.unused: no step, image field or port uses it");
}

#[test]
fn a_setting_only_install_uses_does_nothing_on_the_official_image() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("settings:\n", "settings:\n  mirror: { type: string, values: [a, b], description: A mirror. }\n")
            .replace(
                "    - packages: [demo]\n",
                "    - packages: [demo]\n      when: { set: mirror }\n    - packages: [demo]\n",
            )
    });
    breaks(&files, "settings.mirror: only `install` uses it, which the official image skips");
}

#[test]
fn a_string_reaching_a_template_keeps_to_a_pattern() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("  greeting: { type: string, pattern: \"[a-z ]+\",", "  greeting: { type: string,")
    });
    let files = edited_in(files, "demo/files/demo.conf", |t| format!("{t}say {{{{ with.greeting }}}}\n"));
    let files = edited_in(files, "demo/entry.yml", |t| {
        t.replace("  greeting: { type: string,", "  greeting: { type: string, default: hi,")
    });
    breaks(
        &files,
        "settings.greeting: a string that reaches a template's file keeps to a `pattern` or `values`",
    );
}

/// `files` with `file` changed by `edit`.
fn edited_in(mut files: BTreeMap<String, String>, file: &str, edit: impl FnOnce(&str) -> String) -> BTreeMap<String, String> {
    let text = files.get(file).cloned().unwrap_or_default();
    files.insert(file.to_owned(), edit(&text));
    files
}

#[test]
fn an_entry_holds_only_its_own_files() {
    let mut files = fixture_files().unwrap();
    files.insert("demo/files/vendored.sh".into(), "echo hi\n".into());
    files.insert("demo/vendor/role.yml".into(), "x: 1\n".into());
    breaks(
        &files,
        "catalogue/demo/files/vendored.sh: an entry's file is Isoloom's own: carry `SPDX-License-Identifier: Apache-2.0`",
    );
    breaks(
        &files,
        "catalogue/demo/vendor/role.yml: an entry holds entry.yml, lock.yml, contract.yml and files/ only",
    );
}

#[test]
fn a_template_is_an_entry_file_and_a_copy_the_projects() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("from: files/demo.conf", "from: conf/demo.conf")
            .replace("    - service: demo\n", "    - copy: { from: site/, to: /srv/ }\n    - service: demo\n")
    });
    breaks(&files, "a template's `from` is a file of the entry, under `files/`");
    breaks(&files, "a copy places the project's content a setting names");
}

#[test]
fn a_vendor_package_lock_yml_doesnt_pin_fails_to_resolve() {
    let files = edited("demo/lock.yml", |t| t.replace("      rocky@9: { demo: 2.4.1-1.el9 }\n", ""));
    breaks(
        &files,
        "2.4.1 on rocky@9, the defaults: machines.web.service: demo's recipe (install[2]): lock.yml pins no version of `demo` for 2.4.1 on rocky@9",
    );
}

#[test]
fn an_image_lock_yml_doesnt_pin_fails_to_resolve() {
    let files = edited("demo/lock.yml", |t| {
        t.replace("    images:\n      debian@13:\n", "    images:\n      debian@12:\n")
    });
    breaks(&files, "lock.yml pins no image for 2.4.1 on debian@13");
}

#[test]
fn a_step_names_no_other_machine_and_no_secrets_value() {
    let lab_in_step = edited("demo/entry.yml", |t| {
        t.replace("echo {{ with.greeting }}", "echo {{ with.greeting }} {{ machines.app.addresses.lab }}")
    });
    breaks(&lab_in_step, "the lab's machines are named in a template's file only");
    let value_in_step = edited("demo/entry.yml", |t| t.replace("\"{{ with.admin.path }}\"", "\"{{ with.admin }}\""));
    breaks(
        &value_in_step,
        "`{{ with.admin }}` would write a secret's value: use its file, `{{ with.admin.path }}`",
    );
}

#[test]
fn the_contract_names_only_what_the_entry_has() {
    let names = edited("demo/contract.yml", |t| {
        t.replace("  default: {}\n", "")
            .replace("case: greeting", "case: nope")
            .replace("with: { greeting:", "with: { greet:")
    });
    breaks(&names, "catalogue/demo/contract.yml: cases: a `default: {}` case always runs");
    breaks(&names, "checks[1].case: no case named `nope`");
    breaks(&names, "cases.greeting.with.greet: demo has no setting `greet`");
    let files = edited("demo/contract.yml", |t| {
        t.replace(
            "http: \"http://{{ machine }}:{{ with.port }}/\"",
            "http: \"http://{{ machine }}/\", tcp: \"x:1\"",
        )
    });
    breaks(&files, "a check has exactly one of `http`, `tcp`, `exec` and `script`");
}

#[test]
fn a_daemon_has_one_owner_and_a_form_on_each_family() {
    let mut files = fixture_files().unwrap();
    files.insert(
        "daemons.yml".into(),
        "demo: { unit: demo, foreground: [demo] }\ncron: { unit: { debian: cron, redhat: crond }, foreground: { debian: [cron, -f] } }\n".into(),
    );
    breaks(&files, "the daemon `demo` is defined twice");
    breaks(&files, "daemons.yml: cron: no foreground form on the redhat family, where its unit is");
}

#[test]
fn only_ansibles_own_collections_are_pinned() {
    let mut files = fixture_files().unwrap();
    files.insert(
        "collections.yml".into(),
        format!("collections:\n  - {{ name: community.general, version: '1', url: 'https://galaxy.ansible.com/x.tar.gz', sha256: {}, licence: GPL-3.0-or-later }}\n", "a".repeat(64)),
    );
    breaks(&files, "`community.general` isn't Ansible's own content");
}

#[test]
fn an_entry_without_its_lock_doesnt_load() {
    let mut files = fixture_files().unwrap();
    files.remove("demo/lock.yml");
    let wrong = load(&files).unwrap_err().to_string();
    assert_eq!(wrong, "catalogue/demo: no lock.yml; every entry has entry.yml, lock.yml and contract.yml");
}

#[test]
fn a_settings_fields_fit_its_type() {
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            "settings:\n",
            "settings:\n  a: { type: integer, pattern: '[0-9]+', description: A. }\n  b: { type: string, maximum: 3, description: B. }\n  c: { type: list, description: C. }\n  d: { type: port, owner: demo, description: D. }\n  e: { type: boolean, max: 2, description: E. }\n  f: { type: string, required: true, default: x, description: F. }\n",
        )
    });
    breaks(&files, "settings.a: only `string` settings take `pattern` and `values`");
    breaks(&files, "settings.b: only `integer` settings take `minimum` and `maximum`");
    breaks(&files, "settings.c: a `list` says its items' fields, in `items`");
    breaks(&files, "settings.d: only `password` settings take `owner`");
    breaks(&files, "settings.e: only `list` settings take `items` and `max`");
    breaks(&files, "settings.f: `required` and `default` contradict each other");
}

#[test]
fn the_lock_pins_only_releases_the_entry_has_and_no_distro_package() {
    let files = edited("demo/lock.yml", |t| {
        format!("{t}  \"2.4\":\n    packages:\n      ubuntu@24.04: {{ demo: 2.4.0-1 }}\n  \"9.9\": {{}}\n")
    });
    breaks(
        &files,
        "catalogue/demo/lock.yml: releases.2.4.packages: a `distro` release follows its OS's archive: it pins no package version",
    );
    breaks(&files, "catalogue/demo/lock.yml: releases.9.9: no such release in entry.yml");
}

#[test]
fn a_string_no_sample_fits_is_resolved_with_its_default_or_left_unset() {
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            "settings:\n",
            "settings:\n  code: { type: string, pattern: \"[0-9]+\", description: A code. }\n",
        )
        .replace(
            "    - service: demo\n",
            "    - command: \"demo-code {{ with.code }}\"\n      creates: /etc/demo/code\n      when: { set: code }\n    - service: demo\n",
        )
    });
    assert_eq!(findings(&files), "", "a pattern no made-up value fits isn't sampled");
}

#[test]
fn a_setting_a_step_reads_is_used_and_a_string_outside_templates_needs_no_pattern() {
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            "settings:\n",
            "settings:\n  motd: { type: string, default: hello, description: The message of the day. }\n",
        )
        .replace(
            "    - service: demo\n",
            "    - command: [demo-motd, \"{{ with.motd }}\"]\n      unless: [demo-motd, --check]\n    - service: demo\n",
        )
    });
    assert_eq!(findings(&files), "");
}
