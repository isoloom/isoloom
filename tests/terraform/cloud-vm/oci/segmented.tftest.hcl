# examples/segmented on Oracle Cloud (cloud-vm): three networks as subnets of one VCN, `reach`
# from access to front (every port) and from front to back (6379 only), `back` without internet.
# Each network's security list says what may reach its machines and where they may go (a
# security list can't deny, so the internet is every public block). Every run plans against
# mocked providers; the ids the assertions compare are pinned below.

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
  target          = oci_core_security_list.front
  override_during = plan
  values          = { id = "ocid1.securitylist.oc1..front" }
}

override_resource {
  target          = oci_core_security_list.back
  override_during = plan
  values          = { id = "ocid1.securitylist.oc1..back" }
}

override_resource {
  target          = oci_core_security_list.access
  override_during = plan
  values          = { id = "ocid1.securitylist.oc1..access" }
}

override_resource {
  target          = oci_core_subnet.front
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..front" }
}

override_resource {
  target          = oci_core_subnet.back
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..back" }
}

override_resource {
  target          = oci_core_subnet.access
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..access" }
}

override_resource {
  target          = oci_core_instance.user
  override_during = plan
  values          = { public_ip = "198.51.100.30" }
}

variables {
  compartment_id       = "ocid1.compartment.oc1..tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "each_network_is_a_subnet_of_the_vcn_with_its_own_security_list" {
  command = plan

  assert {
    condition     = sort(oci_core_vcn.env.cidr_blocks) == sort(["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"])
    error_message = "the VCN's blocks must be exactly the three networks'"
  }
  assert {
    condition     = [oci_core_subnet.front.cidr_block, oci_core_subnet.back.cidr_block, oci_core_subnet.access.cidr_block] == ["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"]
    error_message = "each network's subnet must hold the network's exact range"
  }
  assert {
    condition     = oci_core_subnet.front.vcn_id == "ocid1.vcn.oc1..env" && oci_core_subnet.back.vcn_id == "ocid1.vcn.oc1..env" && oci_core_subnet.access.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "every network's subnet must be in the environment's one VCN"
  }
  assert {
    condition     = tolist(oci_core_subnet.front.security_list_ids) == tolist(["ocid1.securitylist.oc1..front"]) && tolist(oci_core_subnet.back.security_list_ids) == tolist(["ocid1.securitylist.oc1..back"]) && tolist(oci_core_subnet.access.security_list_ids) == tolist(["ocid1.securitylist.oc1..access"])
    error_message = "each network's subnet must carry its own security list only"
  }
}

run "the_internet_gateway_is_the_only_way_out" {
  command = plan

  assert {
    condition     = oci_core_internet_gateway.env.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "the internet gateway must be the environment's VCN's (the machines are set up over their public addresses)"
  }
  assert {
    condition     = [for r in oci_core_route_table.env.route_rules : "${r.destination} ${r.network_entity_id}"] == ["0.0.0.0/0 ocid1.internetgateway.oc1..env"]
    error_message = "the environment's route table must hold one rule, the internet through the internet gateway"
  }
  assert {
    condition     = oci_core_subnet.front.route_table_id == "ocid1.routetable.oc1..env" && oci_core_subnet.access.route_table_id == "ocid1.routetable.oc1..env"
    error_message = "front and access (internet: true) must use the environment's route table"
  }
}

run "each_machine_is_on_its_network_at_its_address" {
  command = plan

  assert {
    condition     = one(oci_core_instance.cache.create_vnic_details).subnet_id == "ocid1.subnet.oc1..back" && one(oci_core_instance.cache.create_vnic_details).private_ip == "10.61.20.20"
    error_message = "cache must sit on back at 10.61.20.20"
  }
  assert {
    condition     = one(oci_core_instance.web.create_vnic_details).subnet_id == "ocid1.subnet.oc1..front" && one(oci_core_instance.web.create_vnic_details).private_ip == "10.61.10.10"
    error_message = "web must sit on front at 10.61.10.10"
  }
  assert {
    condition     = one(oci_core_instance.user.create_vnic_details).subnet_id == "ocid1.subnet.oc1..access" && one(oci_core_instance.user.create_vnic_details).private_ip == "10.61.99.10"
    error_message = "user must sit on access at 10.61.99.10"
  }
}

run "back_admits_itself_ssh_and_front_on_6379_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.back.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.61.20.0/24",
      "6 10.61.10.0/24 6379-6379",
      "17 10.61.10.0/24 6379-6379",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "back must admit back, front on 6379 (reach), SSH from allowed_cidr and ICMP's fragmentation-needed, and nothing else"
  }
}

run "front_admits_itself_ssh_and_access_on_every_port" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.front.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.61.10.0/24",
      "all 10.61.99.0/24",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "front must admit front, access on every port (reach), SSH from allowed_cidr and ICMP's fragmentation-needed, and nothing else: nothing from back"
  }
}

run "access_admits_itself_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.access.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.61.99.0/24",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "access must admit access, SSH from allowed_cidr and ICMP's fragmentation-needed only: no reach rule leads into access"
  }
}

run "each_network_goes_out_to_the_others_only_as_reach_allows" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.front.egress_security_rules : join(" ", concat([r.protocol, r.destination], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"])) if startswith(r.destination, "10.")]) == sort([
      "all 10.61.10.0/24",
      "6 10.61.20.0/24 6379-6379",
      "17 10.61.20.0/24 6379-6379",
    ])
    error_message = "front's machines must go out to front, and to back on 6379 (reach), and to no other network"
  }
  assert {
    condition     = [for r in oci_core_security_list.back.egress_security_rules : "${r.protocol} ${r.destination}" if startswith(r.destination, "10.")] == ["all 10.61.20.0/24"]
    error_message = "back's machines must go out to back only, of the environment's networks"
  }
  assert {
    condition     = sort([for r in oci_core_security_list.access.egress_security_rules : "${r.protocol} ${r.destination}" if startswith(r.destination, "10.")]) == sort(["all 10.61.99.0/24", "all 10.61.10.0/24"])
    error_message = "access's machines must go out to access, and to front on every port (reach), and to no other network"
  }
  assert {
    condition     = length([for r in oci_core_security_list.front.egress_security_rules : r.destination if r.protocol == "all" && !startswith(r.destination, "10.")]) == 31 && length([for r in oci_core_security_list.access.egress_security_rules : r.destination if r.protocol == "all" && !startswith(r.destination, "10.")]) == 31
    error_message = "front and access (internet: true) must go out to the internet: the 31 public blocks"
  }
}

run "the_outputs_name_the_way_in_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.30"
    error_message = "`ip` must be the access machine's (user's) public address"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
  assert {
    condition     = keys(output.machines) == ["cache", "user", "web"] && output.ssh_users == { cache = "ubuntu", web = "ubuntu", user = "ubuntu" }
    error_message = "`machines` and `ssh_users` must name every machine"
  }
}

# The spec: `back` has `internet: false`, so its machines may not reach the internet. On Oracle
# Cloud back's subnet uses the environment's route table (0.0.0.0/0 through the internet
# gateway), its security list lets its machines out to every public block, and cache gets a
# public address (it is set up over SSH there): only the guest's own nftables rule
# (`isoloom-egress`) stops it, which root on the machine can lift.
run "known_bug_a_network_without_internet_still_has_a_way_out" {
  command = plan

  assert {
    condition     = length([for r in oci_core_security_list.back.egress_security_rules : r.destination if r.protocol == "all" && !startswith(r.destination, "10.")]) == 0
    error_message = "back (internet: false) must let its machines out to no public block"
  }
  assert {
    condition     = oci_core_subnet.back.route_table_id != "ocid1.routetable.oc1..env"
    error_message = "back (internet: false) must not use the route table whose default rule goes to the internet gateway"
  }
  assert {
    condition     = tostring(one(oci_core_instance.cache.create_vnic_details).assign_public_ip) == "false"
    error_message = "cache, on back (internet: false), must have no public address"
  }
}
