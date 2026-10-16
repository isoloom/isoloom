# examples/solo-web on Oracle Cloud (cloud-vm): one machine on one network, its web port (80,
# http) published as 8080. A network's security list says what may reach its machines; a
# published port is a rule of the machine's own network security group, on its VNIC only. Every
# run plans against mocked providers; the ids and the address the assertions compare are pinned
# below.

mock_provider "oci" {
  override_during = plan
  source          = "./tests/mocks/oci"
}

mock_provider "local" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = oci_core_vcn.env
  override_during = plan
  values          = { id = "ocid1.vcn.oc1..env" }
}

override_resource {
  target          = oci_core_internet_gateway.env
  override_during = plan
  values          = { id = "ocid1.internetgateway.oc1..env" }
}

override_resource {
  target          = oci_core_route_table.env
  override_during = plan
  values          = { id = "ocid1.routetable.oc1..env" }
}

override_resource {
  target          = oci_core_security_list.app
  override_during = plan
  values          = { id = "ocid1.securitylist.oc1..app" }
}

override_resource {
  target          = oci_core_subnet.app
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..app" }
}

override_resource {
  target          = oci_core_network_security_group.web
  override_during = plan
  values          = { id = "ocid1.networksecuritygroup.oc1..web" }
}

override_resource {
  target          = oci_core_instance.web
  override_during = plan
  values          = { public_ip = "198.51.100.10" }
}

variables {
  compartment_id       = "ocid1.compartment.oc1..tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_network_is_a_subnet_of_the_vcn_routed_to_the_internet" {
  command = plan

  assert {
    condition     = oci_core_vcn.env.cidr_block == "10.80.0.0/24" && oci_core_subnet.app.cidr_block == "10.80.0.0/24" && oci_core_subnet.app.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "app must be a subnet at the network's exact range, in the environment's VCN, whose only block it is"
  }
  assert {
    condition     = oci_core_internet_gateway.env.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "the internet gateway must be the environment's VCN's"
  }
  assert {
    condition     = [for r in oci_core_route_table.env.route_rules : "${r.destination} ${r.network_entity_id}"] == ["0.0.0.0/0 ocid1.internetgateway.oc1..env"]
    error_message = "app (internet: true) must route the internet through the internet gateway, and nothing else"
  }
  assert {
    condition     = oci_core_subnet.app.route_table_id == "ocid1.routetable.oc1..env" && tolist(oci_core_subnet.app.security_list_ids) == tolist(["ocid1.securitylist.oc1..app"])
    error_message = "app's subnet must use the environment's route table and app's own security list"
  }
}

run "the_network_admits_itself_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.app.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.80.0.0/24",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "app's security list must admit app, SSH from allowed_cidr and ICMP's fragmentation-needed (path MTU discovery), and nothing else"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_on_its_vnic_only" {
  command = plan

  assert {
    condition     = one(oci_core_instance.web.create_vnic_details).subnet_id == "ocid1.subnet.oc1..app" && one(oci_core_instance.web.create_vnic_details).private_ip == "10.80.0.10"
    error_message = "web must sit on app at 10.80.0.10"
  }
  assert {
    condition     = tolist(one(oci_core_instance.web.create_vnic_details).nsg_ids) == tolist(["ocid1.networksecuritygroup.oc1..web"]) && oci_core_network_security_group.web.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "web's VNIC must be in web's own network security group, in the environment's VCN"
  }
  assert {
    condition     = oci_core_network_security_group_security_rule.web_tcp_8080.network_security_group_id == "ocid1.networksecuritygroup.oc1..web" && oci_core_network_security_group_security_rule.web_tcp_8080.direction == "INGRESS"
    error_message = "the published port's rule must be an ingress rule of web's network security group"
  }
  assert {
    condition     = join(" ", concat([oci_core_network_security_group_security_rule.web_tcp_8080.protocol, oci_core_network_security_group_security_rule.web_tcp_8080.source, oci_core_network_security_group_security_rule.web_tcp_8080.source_type], flatten([for o in oci_core_network_security_group_security_rule.web_tcp_8080.tcp_options : [for p in o.destination_port_range : "${p.min}-${p.max}"]]))) == "6 203.0.113.7/32 CIDR_BLOCK 8080-8080"
    error_message = "web must admit its published port 8080/tcp from allowed_cidr"
  }
}

run "the_published_port_is_an_output_keyed_by_machine_and_port_name" {
  command = plan

  assert {
    condition     = output.published == { "web/web" = "198.51.100.10:8080" }
    error_message = "`published` must hold web's port `web` at its public address and published port"
  }
  assert {
    condition     = output.ip == "198.51.100.10" && output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ip` must be web's public address and `ready_file` the isoloom-ready/1 file"
  }
}
