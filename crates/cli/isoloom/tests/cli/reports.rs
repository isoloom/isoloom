//! `isoloom message` and `isoloom report`: what the environment says to its users, its
//! placeholders filled, and the tables about it.

use crate::testing::{Outcome, isoloom, project};

/// One web container with a `message` naming its address.
const MESSAGED: &str = "version: 1\nname: m\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  web: { networks: { lan: 10 }, ports: { web: 80 }, app: { image: nginx } }\nmessage: \"web at {{ machines.web.addresses.lan }}\"\n";

#[test]
fn message_prints_the_specs_message_with_its_placeholders_filled() -> Outcome {
    let dir = project("message", MESSAGED)?;
    let on_docker: [&dyn AsRef<std::ffi::OsStr>; 4] = [&"message", &dir, &"--target", &"docker"];
    for args in [&on_docker[..2], &on_docker[..]] {
        let o = isoloom(&dir, args)?;
        assert_eq!((o.code, o.stdout.as_str()), (Some(0), "web at 10.9.0.10\n"), "{} arguments: {o:?}", args.len());
    }
    let unknown = isoloom(&dir, &[&"message", &dir, &"--target", &"bogus"])?;
    assert_eq!((unknown.code, unknown.stderr.as_str()), (Some(2), "error: unknown target `bogus`\n"));
    Ok(())
}

#[test]
fn message_says_so_when_the_spec_has_none() -> Outcome {
    let dir = project("no-message", &MESSAGED.replace("message: \"web at {{ machines.web.addresses.lan }}\"\n", ""))?;
    let o = isoloom(&dir, &[&"message", &dir])?;
    assert_eq!((o.code, o.stdout.as_str(), o.stderr.as_str()), (Some(0), "", "the spec has no `message:`\n"));
    Ok(())
}

#[test]
fn report_lists_its_reports_and_prints_each_as_text_or_markdown() -> Outcome {
    let dir = project("reports", MESSAGED)?;
    let list = isoloom(&dir, &[&"report", &"list"])?;
    let names: Vec<&str> = list.stdout.lines().filter_map(|l| l.split_whitespace().next()).collect();
    assert_eq!(names, ["addressing", "ports", "wiring", "resources"], "{list:?}");
    let text = isoloom(&dir, &[&"report", &"addressing", &dir])?;
    let rows: Vec<Vec<&str>> = text.stdout.lines().map(|l| l.split_whitespace().collect()).collect();
    assert_eq!(rows, [vec!["machine", "lan"], vec!["web", "10.9.0.10"]], "{text:?}");
    let md = isoloom(&dir, &[&"report", &"addressing", &dir, &"--md"])?;
    assert_eq!(md.stdout, "| machine | lan |\n| --- | --- |\n| web | 10.9.0.10 |\n", "{md:?}");
    let unknown = isoloom(&dir, &[&"report", &"bogus", &dir])?;
    assert_eq!(unknown.code, Some(2), "{unknown:?}");
    assert_eq!(unknown.stderr, "error: no report named `bogus`; one of: addressing, ports, wiring, resources\n");
    Ok(())
}
