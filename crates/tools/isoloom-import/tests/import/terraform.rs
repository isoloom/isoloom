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

/// A subnet of another size than a network's is read as the network that holds it, with a note:
/// a /30's /29, a /16's first /24. A /30 was once kept as it is, a network the draft's validation
/// refused.
#[test]
fn a_subnet_of_another_size_is_read_as_the_network_holding_it() -> Outcome {
    let tf = |cidr: &str| {
        vec![(
            "main.tf".to_owned(),
            format!(
                "resource \"aws_subnet\" \"lab\" {{\n  cidr_block = \"{cidr}\"\n}}\nresource \"aws_instance\" \"web\" {{\n  ami           = \"ami-1\"\n  instance_type = \"t3.micro\"\n  subnet_id     = aws_subnet.lab.id\n  private_ip    = \"10.40.0.2\"\n}}\n"
            ),
        )]
    };
    for (cidr, kept, why) in [
        ("10.40.0.0/30", "10.40.0.0/29", "smaller than a /29"),
        ("10.40.0.0/16", "10.40.0.0/24", "bigger than a /24"),
    ] {
        let d = draft(&tf(cidr), "subnets", "main.tf")?;
        let spec = spec(&d)?;
        let block: Vec<&str> = spec.networks.values().map(|n| n.cidr.as_str()).collect();
        assert_eq!(block, [kept], "{}", d.yaml);
        assert!(
            has(&d, NoteKind::Changed, "resource aws_subnet.lab", &format!("{cidr} is {why}: kept as {kept}")),
            "{:?}",
            d.notes
        );
    }
    Ok(())
}
