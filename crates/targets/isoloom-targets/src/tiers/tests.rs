use super::*;
use crate::Error;

/// A list with one entry, `entry`, whose runs are `runs` (JSON objects), on the usual hosts.
fn one(entry: &str, goal: u8, exists_on: &str, runs: &str) -> Result<TierEntry> {
    let json = format!(
        r#"{{ "hosts": {{ "mac": "macos", "jarvis": "linux", "karen": "windows" }},
             "entries": [{{ "entry": "{entry}", "goal": {goal}, "exists_on": {exists_on}, "runs": [{runs}] }}] }}"#
    );
    let mut entries = file::parse(&json)?;
    entries.pop().ok_or_else(|| Error::Tiers {
        entry: entry.to_owned(),
        what: "missing".to_owned(),
    })
}

const EVERY_OS: &str = r#"["linux", "macos", "windows"]"#;
const MAC_PASS: &str = r#"{ "host": "mac", "date": "2026-10-15", "result": "pass" }"#;
const JARVIS_PASS: &str = r#"{ "host": "jarvis", "date": "2026-10-16", "result": "pass" }"#;
const KAREN_PASS: &str = r#"{ "host": "karen", "date": "2026-10-16", "result": "pass" }"#;

#[test]
fn the_embedded_list_reads() {
    let entries = tiers().unwrap();
    assert!(entries.len() > Target::ALL.len(), "{} entries", entries.len());
}

#[test]
fn a_target_proven_on_one_host_os_of_three_stays_at_tier_2_with_the_others_pending() {
    let docker = one("docker", 1, EVERY_OS, MAC_PASS).unwrap();
    assert_eq!(docker.tier(), Tier::Two);
    assert_eq!(docker.pending(), BTreeSet::from([HostOs::Linux, HostOs::Windows]));
    assert_eq!(docker.evidence(), "real runs: macos (mac, 2026-10-15); tier 1 pending: linux, windows");
}

#[test]
fn a_passing_run_on_every_host_os_it_exists_on_makes_tier_1() {
    let docker = one("docker", 1, EVERY_OS, &[MAC_PASS, JARVIS_PASS, KAREN_PASS].join(",")).unwrap();
    assert_eq!(docker.tier(), Tier::One);
    assert!(docker.pending().is_empty());
    let libvirt = one("vagrant/libvirt", 1, r#"["linux"]"#, JARVIS_PASS).unwrap();
    assert_eq!((libvirt.tier(), libvirt.id()), (Tier::One, "vagrant/libvirt".to_owned()));
}

#[test]
fn a_later_failing_run_on_a_host_os_takes_tier_1_back() {
    let failed = r#"{ "host": "jarvis", "date": "2026-10-20", "result": "fail" }"#;
    let docker = one("docker", 1, EVERY_OS, &[MAC_PASS, JARVIS_PASS, KAREN_PASS, failed].join(",")).unwrap();
    assert_eq!(docker.tier(), Tier::Two);
    assert_eq!(docker.pending(), BTreeSet::from([HostOs::Linux]));
    assert!(docker.evidence().contains("failed linux (jarvis, 2026-10-20)"), "{}", docker.evidence());
}

#[test]
fn a_partial_run_is_evidence_but_proves_no_host_os() {
    let partial = r#"{ "host": "karen", "date": "2026-10-15", "result": "partial", "note": "up and checks" }"#;
    let hyperv = one("vagrant/hyperv", 1, r#"["windows"]"#, partial).unwrap();
    assert_eq!(hyperv.tier(), Tier::Two);
    assert_eq!(hyperv.evidence(), "real runs: partial windows (karen, 2026-10-15); tier 1 pending: windows");
    assert_eq!(hyperv.json()["runs"][0]["result"], "partial");
}

#[test]
fn an_entry_without_runs_is_validated_only_and_one_not_aiming_for_tier_1_has_nothing_pending() {
    let aws = one("cloud-vm/aws", 1, EVERY_OS, "").unwrap();
    assert_eq!(aws.evidence(), "validated only; tier 1 pending: linux, macos, windows");
    let kubernetes = one("kubernetes", 2, EVERY_OS, "").unwrap();
    assert_eq!((kubernetes.tier(), kubernetes.evidence()), (Tier::Two, "validated only".to_owned()));
}

#[test]
fn a_server_run_names_its_server() {
    let run = r#"{ "host": "mac", "date": "2026-10-15", "result": "pass", "server": "pve-lab" }"#;
    let proxmox = one("proxmox", 1, EVERY_OS, run).unwrap();
    assert!(
        proxmox.evidence().starts_with("real runs: macos (mac, pve-lab, 2026-10-15)"),
        "{}",
        proxmox.evidence()
    );
}

#[test]
fn an_experimental_entry_is_tier_3_whatever_its_runs() {
    let json = r#"{ "hosts": { "mac": "macos" }, "entries": [{ "entry": "vagrant/utm", "goal": 2, "exists_on": ["macos"], "experimental": true,
        "runs": [{ "host": "mac", "date": "2026-10-15", "result": "pass" }] }] }"#;
    let utm = file::parse(json).unwrap().pop().unwrap();
    assert_eq!((utm.tier(), utm.evidence()), (Tier::Three, "experimental".to_owned()));
}

#[test]
fn an_entry_naming_what_doesnt_exist_is_refused_with_its_reason() {
    let refused = |entry: &str, exists_on: &str, runs: &str| match one(entry, 1, exists_on, runs) {
        Err(Error::Tiers { what, .. }) => what,
        other => panic!("{entry}: {other:?}"),
    };
    assert!(refused("dockre", EVERY_OS, "").contains("no such target"));
    assert!(refused("vagrant/proxmox", EVERY_OS, "").contains("no such target"));
    assert!(refused("vagrant", EVERY_OS, "").contains("no such target"), "a target with providers needs one");
    assert!(refused("docker", "[]", "").contains("names no host OS"));
    assert!(refused("docker", EVERY_OS, r#"{ "host": "friday", "date": "2026-10-15", "result": "pass" }"#).contains("`friday`"));
    assert!(refused("docker", EVERY_OS, r#"{ "host": "mac", "date": "15/10/2026", "result": "pass" }"#).contains("isn't a date"));
    assert!(refused("vagrant/hyperv", r#"["windows"]"#, MAC_PASS).contains("where it doesn't exist"));
    assert!(matches!(one("docker", 4, EVERY_OS, ""), Err(Error::Tiers { what, .. }) if what.contains("goal 4")));
    assert!(matches!(file::parse("{}"), Err(Error::TiersJson(_))));
}

#[test]
fn a_date_is_a_calendar_day_written_yyyy_mm_dd() {
    assert_eq!(Date::parse("2026-10-15").map(|d| d.to_string()), Some("2026-10-15".to_owned()));
    for not in [
        "2026-13-01",
        "2026-10-32",
        "2026-1-15",
        "26-10-15",
        "2026-10-15-1",
        "2026-10",
        "2026-00-10",
        "x",
    ] {
        assert_eq!(Date::parse(not), None, "{not}");
    }
    assert!(Date::parse("2026-10-15") < Date::parse("2026-10-16"));
}

#[test]
fn every_target_has_its_entries_and_each_provider_or_cloud_one() {
    let entries = tiers().unwrap();
    let ids: Vec<String> = entries.iter().map(TierEntry::id).collect();
    let expected: Vec<String> = Target::ALL
        .into_iter()
        .flat_map(|t| match variants(t) {
            v if v.is_empty() => vec![t.id().to_owned()],
            v => v.into_iter().map(|v| format!("{}/{v}", t.id())).collect(),
        })
        .collect();
    assert_eq!(ids, expected);
    for t in Target::ALL {
        assert!(!of(&entries, t).is_empty(), "{}", t.id());
    }
}
