//! The readers the Isoloom crates' tests parse rendered files with, as those tests call them:
//! HCL by block and attribute, YAML by path, Ruby by `do … end` block, and `assert_every!`.

use isoloom_test_support::{
    assert_every, hcl_blocks, hcl_body, hcl_names, hcl_one, hcl_text, hcl_values, manifest, manifest_names, ruby_blocks, yaml, yaml_at, yaml_documents,
    yaml_keys_at, yaml_str, yaml_strs,
};

const TF: &str = r#"
resource "aws_instance" "web" {
  instance_type = "t3.micro"
  root_block_device { volume_size = 20 }
  tags = { Name = "web" }
}
resource "aws_instance" "db" { instance_type = "t3.small" }
variable "region" { default = "eu-west-3" }
"#;

#[test]
fn hcl_is_read_by_block_and_attribute() {
    let body = hcl_body(TF);
    assert_eq!(hcl_names(&body, "resource", "aws_instance"), ["web", "db"]);
    assert_eq!(hcl_blocks(&body, "variable", &["region"]).len(), 1);
    let web = hcl_one(&body, "aws_instance", "web");
    assert_eq!(hcl_text(web, "instance_type"), "t3.micro");
    // Nested blocks too, by name.
    assert_eq!(hcl_values(web, "volume_size"), ["20"]);
}

#[test]
fn yaml_is_read_by_path_lists_and_keys() {
    let c = yaml("services:\n  web:\n    image: nginx\n    ports: ['8080:80', '8443:443']\n    labels: { isoloom.port.web: 'tcp:80' }\n");
    assert_eq!(yaml_str(&c, "services.web.image"), "nginx");
    assert_eq!(yaml_strs(&c, "services.web.ports"), ["8080:80", "8443:443"]);
    assert_eq!(yaml_str(&c, "services.web.ports.1"), "8443:443");
    assert_eq!(yaml_str(&c, "services.web.labels.`isoloom.port.web`"), "tcp:80");
    assert_eq!(yaml_keys_at(&c, "services.web"), ["image", "ports", "labels"]);
    assert!(yaml_at(&c, "services.db").is_none());
}

#[test]
fn kubernetes_manifests_are_found_by_kind_and_name() {
    let docs = yaml_documents(
        "kind: Service\nmetadata: { name: web }\n---\nkind: Deployment\nmetadata: { name: web }\n---\nkind: Deployment\nmetadata: { name: db }\n",
    );
    assert_eq!(manifest_names(&docs, "Deployment"), ["web", "db"]);
    assert_eq!(manifest(&docs, "Service", "web")["kind"].as_str(), Some("Service"));
}

#[test]
fn ruby_is_read_as_nested_blocks_a_here_document_inside_its_statement() {
    let rb = "Vagrant.configure(\"2\") do |config|\n  config.vm.define \"web\" do |m|\n    m.vm.box = \"bento/debian-12\"\n    m.vm.provision \"shell\", inline: <<~SH\n      echo do\n    SH\n  end\nend\n";
    let file = ruby_blocks(rb);
    let web = file.block("config.vm.define \"web\"");
    assert_eq!(web.statements("m.vm.box"), ["m.vm.box = \"bento/debian-12\""]);
    let provision = web.statements("m.vm.provision");
    assert_eq!(provision.len(), 1);
    assert!(provision[0].ends_with("SH"), "the here-document is the statement's: {provision:?}");
    assert!(web.blocks.is_empty(), "`echo do` opens no block");
}

#[test]
fn every_item_holds_or_the_first_that_doesnt_is_named() {
    assert_every!([1, 2, 3], |n: &i32| *n > 0);
}
