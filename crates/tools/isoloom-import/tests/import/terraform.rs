//! `isoloom import terraform`: the `.tf` files of a folder, read statically.

use isoloom_import::NoteKind;
use isoloom_import::terraform::draft;
use isoloom_spec::syntax::Octets;

use crate::testing::{Outcome, fixture, has, spec, validate_in};

/// The `.tf` files of the fixture folder `name`, as (name, text), sorted.
fn tf_files(name: &str) -> Outcome<Vec<(String, String)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(fixture(name))? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "tf") {
            let file = path.file_name().ok_or("a file with no name")?.to_string_lossy().into_owned();
            files.push((file, std::fs::read_to_string(&path)?));
        }
    }
    files.sort();
    Ok(files)
}

#[test]
fn for_each_machines_through_their_network_interfaces_make_a_draft_that_passes_validate() -> Outcome {
    let d = draft(&tf_files("terraform-aws")?, "aws-lab", "main.tf")?;
    assert_eq!(validate_in(&d, &fixture("terraform-aws"))?, Vec::<String>::new(), "{}", d.yaml);
    let spec = spec(&d)?;
    assert_eq!(spec.networks["lab-private"].cidr, "192.168.56.0/24", "the subnet, through a local");
    assert_eq!(
        spec.machines["dc01"].networks["lab-private"],
        Octets::One(10),
        "the address on its network interface"
    );
    assert_eq!(spec.machines["srv02"].networks["lab-private"], Octets::One(22));
    assert_eq!(spec.machines["srv02"].resources.and_then(|r| r.memory_mb), Some(8192), "t3.large");
    assert_eq!(spec.machines["dc01"].os.map(|o| o.to_string()).as_deref(), Some("windows-server@2019"));
    assert!(has(&d, NoteKind::Changed, "resource aws_s3_bucket", "not read by the import: left out"));
    assert!(has(&d, NoteKind::Changed, "resource aws_vpc", "not read by the import: left out"));
    assert!(!d.notes.iter().any(|n| n.at == "resource aws_subnet"), "a subnet is read, not left out");
    Ok(())
}

#[test]
fn not_terraform_is_an_error() {
    let bad = vec![("main.tf".to_owned(), "resource \"x\" {".to_owned())];
    assert!(matches!(draft(&bad, "x", "main.tf"), Err(isoloom_import::Error::Hcl { .. })));
    let none = vec![("main.tf".to_owned(), "resource \"aws_s3_bucket\" \"b\" {}\n".to_owned())];
    assert!(
        matches!(draft(&none, "x", "main.tf"), Err(isoloom_import::Error::NoTerraformVm)),
        "no VM: nothing to draft"
    );
}
