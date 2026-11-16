//! A broken entry fails to load, with every rule it breaks at its file and field: the
//! catalogue's tests catch it, never a user's spec.

use std::collections::BTreeMap;

use isoloom_catalogue::{Error, Finding};

use crate::testing::{fixture_files, load};

/// The fixture catalogue with `file` (`demo/entry.yml`) changed by `edit`.
fn edited(file: &str, edit: impl FnOnce(&str) -> String) -> BTreeMap<String, String> {
    edited_in(fixture_files().unwrap_or_default(), file, edit)
}

/// Every finding of `files`: none when they load; a failure to load that isn't a broken rule,
/// as one finding that no rule's test expects.
fn findings(files: &BTreeMap<String, String>) -> Vec<Finding> {
    match load(files) {
        Err(Error::Invalid(found)) => found,
        Err(other) => vec![Finding::new("not a broken rule", other.to_string())],
        Ok(_) => Vec::new(),
    }
}

/// `files` break a rule: a finding at `at` (`demo/entry.yml: settings.a`) says `reason`, whole.
fn breaks(files: &BTreeMap<String, String>, at: &str, reason: &str) {
    let found = findings(files);
    assert!(
        found.iter().any(|f| f.at == at && f.reason == reason),
        "expected `{at}`: `{reason}` in:\n{}",
        found.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
    );
}

/// `files` break a rule of the recipe when it is resolved, first in `context` (`2.4.1 on
/// debian@12, the defaults`) at the step `step` (`configure[4]`), saying `reason`.
fn breaks_resolving(files: &BTreeMap<String, String>, context: &str, step: &str, reason: &str) {
    breaks(
        files,
        "demo/entry.yml: recipe",
        &format!("{context}: machines.web.service: demo's recipe ({step}): {reason}"),
    );
}

#[test]
fn the_fixture_breaks_no_rule() {
    assert_eq!(findings(&fixture_files().unwrap()), []);
}

#[test]
fn the_default_is_one_of_the_releases() {
    breaks(
        &edited("demo/entry.yml", |t| t.replace("default: 2.4.1", "default: 9.9")),
        "demo/entry.yml: default",
        "`9.9` isn't one of the releases (2.4.1 (stable), 2.5.0 (edge), 2.4 (os))",
    );
}

#[test]
fn a_setting_nothing_uses_would_do_nothing() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("settings:\n", "settings:\n  unused: { type: boolean, description: Nothing. }\n")
    });
    breaks(
        &files,
        "demo/entry.yml: settings.unused",
        "no step, image field or port uses it: it would do nothing",
    );
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
    breaks(
        &files,
        "demo/entry.yml: settings.mirror",
        "only `install` uses it, which the official image skips: use it in `configure`, `start` or `image` too",
    );
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
        "demo/entry.yml: settings.greeting",
        "a string that reaches a template's file keeps to a `pattern` or `values`, so nothing is injected through it",
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
        "demo/files/vendored.sh",
        "an entry's file is Isoloom's own: carry `SPDX-License-Identifier: Apache-2.0`",
    );
    breaks(
        &files,
        "demo/vendor/role.yml",
        "an entry holds entry.yml, lock.yml, contract.yml and files/ only: nothing third-party",
    );
}

#[test]
fn a_template_is_an_entry_file_and_a_copy_the_projects() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("from: files/demo.conf", "from: conf/demo.conf")
            .replace("    - service: demo\n", "    - copy: { from: site/, to: /srv/ }\n    - service: demo\n")
    });
    breaks(
        &files,
        "demo/entry.yml: recipe.configure[0]",
        "a template's `from` is a file of the entry, under `files/` (`files/site.conf`), not `conf/demo.conf`",
    );
    breaks(
        &files,
        "demo/entry.yml: recipe.configure[4]",
        "a copy places the project's content a setting names (`{{ with.root }}/`), not `site/`: the entry's own files go through `template`",
    );
}

/// Regression: a copy from `{{with.root}}/` (no spaces) was refused, though the placeholder
/// grammar reads it as `{{ with.root }}`.
#[test]
fn a_copy_from_a_setting_is_taken_however_its_braces_are_spaced() {
    for from in ["{{ with.root }}/", "{{with.root}}/", "{{  with.root}}/site/"] {
        let files = edited("demo/entry.yml", |t| {
            t.replace(
                "    - service: demo\n",
                &format!("    - copy: {{ from: \"{from}\", to: /srv/ }}\n    - service: demo\n"),
            )
        });
        let at_the_copy: Vec<_> = findings(&files).into_iter().filter(|f| f.at == "demo/entry.yml: recipe.configure[4]").collect();
        assert_eq!(at_the_copy, [], "{from}");
    }
}

#[test]
fn a_vendor_package_lock_yml_doesnt_pin_fails_to_resolve() {
    let files = edited("demo/lock.yml", |t| t.replace("      rocky@9: { demo: 2.4.1-1.el9 }\n", ""));
    breaks_resolving(
        &files,
        "2.4.1 on rocky@9, the defaults",
        "install[2]",
        "lock.yml pins no version of `demo` for 2.4.1 on rocky@9: run `cargo xtask catalogue refresh demo`",
    );
}

#[test]
fn an_image_lock_yml_doesnt_pin_fails_to_resolve() {
    let files = edited("demo/lock.yml", |t| {
        t.replace("    images:\n      debian@13:\n", "    images:\n      debian@12:\n")
    });
    breaks(
        &files,
        "demo/entry.yml: recipe",
        "2.4.1 on debian@13, the defaults: machines.web.service: demo: lock.yml pins no image for 2.4.1 on debian@13: run `cargo xtask catalogue refresh demo`",
    );
}

#[test]
fn a_step_names_no_other_machine_and_no_secrets_value() {
    let lab_in_step = edited("demo/entry.yml", |t| {
        t.replace("echo {{ with.greeting }}", "echo {{ with.greeting }} {{ machines.app.addresses.lab }}")
    });
    breaks_resolving(
        &lab_in_step,
        "2.4.1 on debian@12, `greeting` set",
        "configure[1]",
        "`{{ machines.app.addresses.lab }}`: the environment's machines are named in a template's file only: a step runs before the machine reaches the others",
    );
    let value_in_step = edited("demo/entry.yml", |t| t.replace("\"{{ with.admin.path }}\"", "\"{{ with.admin }}\""));
    breaks_resolving(
        &value_in_step,
        "2.4.1 on debian@12, `admin` set",
        "start[0]",
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
    breaks(&names, "demo/contract.yml: cases", "a `default: {}` case always runs: add it");
    breaks(&names, "demo/contract.yml: checks[1].case", "no case named `nope`");
    breaks(&names, "demo/contract.yml: cases.greeting.with.greet", "demo has no setting `greet`");
    let files = edited("demo/contract.yml", |t| {
        t.replace(
            "http: \"http://{{ machine }}:{{ with.port }}/\"",
            "http: \"http://{{ machine }}/\", tcp: \"x:1\"",
        )
    });
    breaks(
        &files,
        "demo/contract.yml: checks[0]",
        "a check has exactly one of `http`, `tcp`, `exec` and `script`",
    );
}

#[test]
fn a_daemon_has_one_owner_and_a_form_on_each_family() {
    let mut files = fixture_files().unwrap();
    files.insert(
        "daemons.yml".into(),
        "demo: { unit: demo, foreground: [demo] }\ncron: { unit: { debian: cron, redhat: crond }, foreground: { debian: [cron, -f] } }\n".into(),
    );
    breaks(
        &files,
        "daemons.yml",
        "the daemon `demo` is defined twice: each daemon has one owner (its entry, else daemons.yml)",
    );
    breaks(&files, "daemons.yml: cron", "no foreground form on the redhat family, where its unit is");
}

#[test]
fn only_ansibles_own_collections_are_pinned() {
    let mut files = fixture_files().unwrap();
    files.insert(
        "collections.yml".into(),
        format!("collections:\n  - {{ name: community.general, version: '1', url: 'https://galaxy.ansible.com/x.tar.gz', sha256: {}, licence: GPL-3.0-or-later }}\n", "a".repeat(64)),
    );
    breaks(
        &files,
        "collections.yml",
        "`community.general` isn't Ansible's own content: only `ansible.*` collections are pinned (never `community.*` or a vendor's)",
    );
}

#[test]
fn an_entry_without_its_lock_doesnt_load() {
    let mut files = fixture_files().unwrap();
    files.remove("demo/lock.yml");
    let wrong = load(&files).unwrap_err();
    assert!(matches!(&wrong, Error::Missing { entry, file: "lock.yml" } if entry == "demo"), "{wrong}");
    assert_eq!(
        wrong.to_string(),
        "catalogue/demo: no lock.yml; every entry has entry.yml, lock.yml and contract.yml"
    );
}

#[test]
fn a_settings_fields_fit_its_type() {
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            "settings:\n",
            "settings:\n  a: { type: integer, pattern: '[0-9]+', description: A. }\n  b: { type: string, maximum: 3, description: B. }\n  c: { type: list, description: C. }\n  d: { type: port, owner: demo, description: D. }\n  e: { type: boolean, max: 2, description: E. }\n  f: { type: string, required: true, default: x, description: F. }\n",
        )
    });
    breaks(&files, "demo/entry.yml: settings.a", "only `string` settings take `pattern` and `values`");
    breaks(&files, "demo/entry.yml: settings.b", "only `integer` settings take `minimum` and `maximum`");
    breaks(&files, "demo/entry.yml: settings.c", "a `list` says its items' fields, in `items`");
    breaks(&files, "demo/entry.yml: settings.d", "only `password` settings take `owner`");
    breaks(&files, "demo/entry.yml: settings.e", "only `list` settings take `items` and `max`");
    breaks(&files, "demo/entry.yml: settings.f", "`required` and `default` contradict each other");
}

#[test]
fn the_lock_pins_only_releases_the_entry_has_and_no_distro_package() {
    let files = edited("demo/lock.yml", |t| {
        format!("{t}  \"2.4\":\n    packages:\n      ubuntu@24.04: {{ demo: 2.4.0-1 }}\n  \"9.9\": {{}}\n")
    });
    breaks(
        &files,
        "demo/lock.yml: releases.2.4.packages",
        "a `distro` release follows its OS's archive: it pins no package version",
    );
    breaks(
        &files,
        "demo/lock.yml: releases.9.9",
        "no such release in entry.yml: run `cargo xtask catalogue refresh`",
    );
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
    assert_eq!(findings(&files), [], "a pattern no made-up value fits isn't sampled");
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
    assert_eq!(findings(&files), []);
}

#[test]
fn what_the_tools_write_beside_the_entries_isnt_catalogue_data() {
    let mut files = fixture_files().unwrap();
    files.insert("conformance.json".into(), "{ \"runs\": [] }\n".into());
    files.insert("entry.schema.json".into(), "{ \"title\": \"entry.yml\" }\n".into());
    assert_eq!(findings(&files), []);
}

/// Inserts `steps` into the demo entry's `configure`, before its `service` step.
fn demo_with_steps(steps: &str) -> BTreeMap<String, String> {
    edited("demo/entry.yml", |t| t.replace("    - service: demo", &format!("{steps}    - service: demo")))
}

#[test]
fn a_recipe_key_whose_fingerprint_lock_yml_doesnt_pin_fails() {
    let files = edited("demo/lock.yml", |t| {
        t.replace("0123456789ABCDEF0123456789ABCDEF01234567", "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
    });
    breaks_resolving(
        &files,
        "2.4.1 on debian@12, the defaults",
        "install[0]",
        "lock.yml pins no key `https://example.org/key.asc` with fingerprint \"0123456789ABCDEF0123456789ABCDEF01234567\": run `cargo xtask catalogue refresh demo`",
    );
}

#[test]
fn a_vendor_package_with_no_pinned_version_is_left_as_written() {
    // `extra` has no version and isn't in lock.yml: a vendor release leaves it for the OS.
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            r#"packages: [{ name: demo, version: "{{ version }}" }]"#,
            r#"packages: [{ name: demo, version: "{{ version }}" }, extra]"#,
        )
    });
    assert_eq!(findings(&files), []);
}

#[test]
fn a_list_setting_in_a_text_is_read_item_by_item() {
    let files = demo_with_steps("    - command: \"echo {{ with.users }}\"\n      when: { set: users }\n");
    breaks_resolving(
        &files,
        "2.4.1 on debian@12, `users` set",
        "configure[4]",
        "`{{ with.users }}`: a list is read item by item, with `for_each`",
    );
}

#[test]
fn an_item_fields_file_path_is_read_from_the_named_field() {
    let files = demo_with_steps("    - command: \"cat {{ item.password.path }}\"\n      creates: /etc/demo/pw\n      for_each: users\n");
    assert_eq!(findings(&files), []);
}

#[test]
fn a_lab_machine_reference_is_refused_outside_a_template() {
    let files = demo_with_steps("    - command: \"echo {{ machine.name }}\"\n");
    breaks_resolving(
        &files,
        "2.4.1 on debian@12, the defaults",
        "configure[4]",
        "`{{ machine.name }}`: the environment's machines are named in a template's file only: a step runs before the machine reaches the others",
    );
}

#[test]
fn a_whole_placeholder_of_a_number_setting_keeps_its_type() {
    // `uid` is an integer; `{{ with.port }}` alone fills in the number, not the string "8080".
    let files = demo_with_steps("    - user: { name: svc, uid: \"{{ with.port }}\" }\n");
    assert_eq!(findings(&files), []);
}

#[test]
fn each_field_of_an_entry_is_checked_where_it_is() {
    let files = edited("demo/entry.yml", |t| {
        t.replace(
            "description: A made-up service for the catalogue's own tests.",
            "description: \" \"",
        )
        .replace("    os: [debian@12]\n    vars: { path: edge }", "    os: []\n    vars: { path: edge }")
        .replace(
            "  debian@13: docker.io/library/demo:{{ version }}\n",
            "  debian@13: docker.io/library/demo:latest\n  alpine@3.24: docker.io/library/demo:{{ version }}\n",
        )
        .replace("  peer: { type: endpoint, description: A peer. }", "  peer: { type: endpoint, description: \"\" }")
        .replace(
            "      password: { type: password, description: Its password. }",
            "      password: { type: password, description: Its password. }\n      keys: { type: list, items: { k: { type: string, description: K. } }, description: Keys. }",
        )
    });
    let files = edited_in(files, "demo/contract.yml", |t| t.replace("exec: demo --version", "exec: \" \""));
    for (at, reason) in [
        ("demo/entry.yml: description", "say what the service is, for the catalogue page"),
        ("demo/entry.yml: releases[1].os", "list the OSes it runs on"),
        (
            "demo/entry.yml: images.debian@13",
            "write `{{ version }}` where the release goes, so each release has its own image",
        ),
        ("demo/entry.yml: images.alpine@3.24", "no release runs on this OS"),
        ("demo/entry.yml: settings.peer", "say what it sets, in `description`"),
        ("demo/entry.yml: settings.users", "item field `keys`: a list's items don't nest lists"),
        ("demo/contract.yml: facts[0].exec", "a fact is a command whose output both forms share"),
    ] {
        breaks(&files, at, reason);
    }
}

#[test]
fn a_release_listed_twice_and_a_port_read_from_another_kind_of_setting_are_refused() {
    let files = edited("demo/entry.yml", |t| {
        t.replace("  - version: 2.5.0\n", "  - version: 2.4.1\n")
            .replace("  admin: { port: 9090 }", "  admin: { setting: greeting }")
    });
    breaks(&files, "demo/entry.yml: releases[1]", "release 2.4.1 is listed twice");
    breaks(&files, "demo/entry.yml: ports.admin.setting", "`greeting` isn't a `port` setting");
}

#[test]
fn a_codename_is_read_only_on_an_os_that_has_one() {
    let files = edited("demo/entry.yml", |t| t.replace("/rpm/{{ os.version }}", "/rpm/{{ os.codename }}"));
    breaks_resolving(
        &files,
        "2.4.1 on rocky@9, the defaults",
        "install[1]",
        "`{{ os.codename }}`: the OS has no codename (only apt's releases have one)",
    );
}
