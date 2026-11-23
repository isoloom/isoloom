//! `isoloom import terraform`: the `.tf` files of a folder, read statically.

use isoloom_import::NoteKind;
use isoloom_import::terraform::draft;
use isoloom_spec::syntax::Octets;

use crate::testing::{Outcome, fixture, has, spec, validate, validate_in};

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
    assert!(has(
        &d,
        NoteKind::Changed,
        "resource aws_s3_bucket",
        "not read by the import: left out; carry it over by hand if the environment needs it"
    ));
    assert!(has(
        &d,
        NoteKind::Changed,
        "resource aws_vpc",
        "not read by the import: left out; carry it over by hand if the environment needs it"
    ));
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

/// A `count` that isn't a whole number drafts no machine, and says so: it once drafted none
/// without a word.
#[test]
fn a_count_that_isnt_a_whole_number_is_said() -> Outcome {
    let tf = vec![(
        "main.tf".to_owned(),
        "resource \"aws_instance\" \"web\" {\n  count         = -1\n  ami           = \"ami-1\"\n  instance_type = \"t3.micro\"\n}\nresource \"aws_instance\" \"db\" {\n  ami           = \"ami-1\"\n  instance_type = \"t3.micro\"\n}\n".to_owned(),
    )];
    let d = draft(&tf, "counted", "main.tf")?;
    assert!(
        has(
            &d,
            NoteKind::Changed,
            "resource aws_instance.web",
            "its `count` is -1, not a whole number of machines: add these machines by hand"
        ),
        "{:?}",
        d.notes
    );
    assert_eq!(spec(&d)?.machines.keys().collect::<Vec<_>>(), ["db"], "no machine drafted for it");
    Ok(())
}

#[test]
fn azure_google_and_proxmox_vms_are_read_with_their_os_size_and_address() -> Outcome {
    let d = draft(&tf_files("terraform-clouds")?, "clouds", "main.tf")?;
    assert_eq!(validate(&d)?, Vec::<String>::new(), "{}", d.yaml);
    let spec = spec(&d)?;
    let read: Vec<String> = spec
        .machines
        .iter()
        .map(|(name, m)| {
            let os = m.os.map(|o| o.to_string()).unwrap_or_default();
            let size = m.resources.map(|r| format!(" {:?} CPUs {:?} MB", r.cpus, r.memory_mb)).unwrap_or_default();
            format!("{name} {os} at {:?}{size}", m.networks["net"])
        })
        .collect();
    assert_eq!(
        read,
        [
            "dc windows-server@2019 at One(10)",
            "web debian@11 at One(20) Some(4) CPUs Some(16384) MB",
            "db debian@12 at One(30) Some(2) CPUs Some(2048) MB",
        ],
        "{}",
        d.yaml
    );
    assert_eq!(spec.networks["net"].cidr, "10.50.1.0/24");
    assert!(
        has(
            &d,
            NoteKind::Changed,
            "resource azurerm_windows_virtual_machine.dc.size",
            "`Standard_B9`: set `resources` by hand"
        ),
        "an unknown size is said: {:?}",
        d.notes
    );
    Ok(())
}

#[test]
fn a_list_for_each_makes_a_machine_per_item_and_what_needs_terraform_to_read_is_said() -> Outcome {
    let tf = vec![(
        "main.tf".to_owned(),
        r#"
variable "names" { default = ["a", "b"] }
resource "aws_subnet" "public" { cidr_block = "8.8.8.0/24" }
resource "aws_instance" "each" {
  for_each      = var.names
  ami           = "ami-1"
  instance_type = "t3.micro"
}
resource "aws_instance" "later" {
  for_each = data.aws_ami.all.ids
  ami      = "ami-1"
}
resource "aws_instance" "counted" {
  count = length(data.aws_ami.all.ids)
  ami   = "ami-1"
}
"#
        .to_owned(),
    )];
    let d = draft(&tf, "expanded", "main.tf")?;
    assert_eq!(spec(&d)?.machines.keys().collect::<Vec<_>>(), ["a", "b"], "one per item, named by it");
    let said: Vec<(&str, &str)> = d
        .notes
        .iter()
        .filter(|n| !n.at.starts_with("resource aws_instance.each"))
        .map(|n| (n.at.as_str(), n.text.as_str()))
        .collect();
    assert_eq!(
        said,
        [
            (
                "resource aws_instance.later",
                "its `for_each` couldn't be read without running Terraform: add these machines by hand"
            ),
            ("resource aws_instance.counted", "its `count` couldn't be read without running Terraform"),
            ("resource aws_subnet.public", "subnet `8.8.8.0/24` couldn't be read as a private block"),
        ]
    );
    Ok(())
}
