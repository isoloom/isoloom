# examples/edge-firewall on Oracle Cloud (cloud-vm): `fw` routes dmz and lan (`gateway: fw`), so
# traffic between the networks crosses it; `reach` opens outside to dmz on 80 and dmz to lan on
# 6379; lan has no internet. Oracle Cloud keeps a subnet's first address, so fw takes .254 on the
# networks it routes, on a VNIC of its own there; a route names fw's private IP there. Every run
# plans against mocked providers; the ids the assertions compare are pinned below.

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
  target          = oci_core_route_table.outside
  override_during = plan
  values          = { id = "ocid1.routetable.oc1..outside" }
}

override_resource {
  target          = oci_core_route_table.dmz
  override_during = plan
  values          = { id = "ocid1.routetable.oc1..dmz" }
}

override_resource {
  target          = oci_core_route_table.lan
  override_during = plan
  values          = { id = "ocid1.routetable.oc1..lan" }
}

override_resource {
  target          = oci_core_subnet.outside
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..outside" }
}

override_resource {
  target          = oci_core_subnet.dmz
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..dmz" }
}

override_resource {
  target          = oci_core_subnet.lan
  override_during = plan
  values          = { id = "ocid1.subnet.oc1..lan" }
}

override_resource {
  target          = oci_core_instance.fw
  override_during = plan
  values          = { id = "ocid1.instance.oc1..fw" }
}

variables {
  compartment_id       = "ocid1.compartment.oc1..tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "every_network_is_a_subnet_of_the_vcn_at_its_exact_range" {
  command = plan

  assert {
    condition     = sort(oci_core_vcn.env.cidr_blocks) == sort(["10.70.0.0/24", "10.70.10.0/24", "10.70.20.0/24"])
    error_message = "the VCN's blocks must be exactly the three networks'"
  }
  assert {
    condition     = [oci_core_subnet.outside.cidr_block, oci_core_subnet.dmz.cidr_block, oci_core_subnet.lan.cidr_block] == ["10.70.0.0/24", "10.70.10.0/24", "10.70.20.0/24"]
    error_message = "each network's subnet must hold the network's exact range"
  }
}

run "each_network_has_its_table_sending_the_others_through_fw" {
  command = plan

  assert {
    condition     = [oci_core_route_table_attachment.outside.subnet_id, oci_core_route_table_attachment.outside.route_table_id] == ["ocid1.subnet.oc1..outside", "ocid1.routetable.oc1..outside"]
    error_message = "outside's subnet must move to outside's own route table"
  }
  assert {
    condition     = [oci_core_route_table_attachment.dmz.subnet_id, oci_core_route_table_attachment.dmz.route_table_id] == ["ocid1.subnet.oc1..dmz", "ocid1.routetable.oc1..dmz"]
    error_message = "dmz's subnet must move to dmz's own route table"
  }
  assert {
    condition     = [oci_core_route_table_attachment.lan.subnet_id, oci_core_route_table_attachment.lan.route_table_id] == ["ocid1.subnet.oc1..lan", "ocid1.routetable.oc1..lan"]
    error_message = "lan's subnet must move to lan's own route table"
  }
  assert {
    condition     = sort([for r in oci_core_route_table.outside.route_rules : r.destination]) == sort(["0.0.0.0/0", "10.70.10.0/24", "10.70.20.0/24"]) && sort([for r in oci_core_route_table.dmz.route_rules : r.destination]) == sort(["0.0.0.0/0", "10.70.0.0/24", "10.70.20.0/24"]) && sort([for r in oci_core_route_table.lan.route_rules : r.destination]) == sort(["0.0.0.0/0", "10.70.0.0/24", "10.70.10.0/24"])
    error_message = "each network's table must route the internet and every other network"
  }
  # A route to another network names fw's private IP there, which a data source finds once fw's
  # VNIC exists: the plan says which address it looks up, on which subnet, not the id it finds.
  assert {
    condition     = [data.oci_core_private_ips.fw_outside.ip_address, data.oci_core_private_ips.fw_outside.subnet_id] == ["10.70.0.5", "ocid1.subnet.oc1..outside"] && [data.oci_core_private_ips.fw_dmz.ip_address, data.oci_core_private_ips.fw_dmz.subnet_id] == ["10.70.10.254", "ocid1.subnet.oc1..dmz"] && [data.oci_core_private_ips.fw_lan.ip_address, data.oci_core_private_ips.fw_lan.subnet_id] == ["10.70.20.254", "ocid1.subnet.oc1..lan"]
    error_message = "each network's routes to the others must go through fw's address in it: 10.70.0.5 on outside, 10.70.10.254 on dmz, 10.70.20.254 on lan"
  }
  assert {
    condition     = [for r in oci_core_route_table.outside.route_rules : r.network_entity_id if r.destination == "0.0.0.0/0"] == ["ocid1.internetgateway.oc1..env"] && [for r in oci_core_route_table.dmz.route_rules : r.network_entity_id if r.destination == "0.0.0.0/0"] == ["ocid1.internetgateway.oc1..env"]
    error_message = "outside and dmz (internet: true) must route the internet through the internet gateway"
  }
}

run "fw_forwards_on_a_vnic_per_network" {
  command = plan

  assert {
    condition     = one(oci_core_instance.fw.create_vnic_details).subnet_id == "ocid1.subnet.oc1..outside" && one(oci_core_instance.fw.create_vnic_details).private_ip == "10.70.0.5"
    error_message = "fw's first VNIC must be on outside at 10.70.0.5"
  }
  assert {
    condition     = one(oci_core_vnic_attachment.fw_dmz.create_vnic_details).subnet_id == "ocid1.subnet.oc1..dmz" && one(oci_core_vnic_attachment.fw_dmz.create_vnic_details).private_ip == "10.70.10.254" && oci_core_vnic_attachment.fw_dmz.instance_id == "ocid1.instance.oc1..fw"
    error_message = "fw must have a VNIC on dmz at 10.70.10.254 (Oracle Cloud keeps the gateway address .1)"
  }
  assert {
    condition     = one(oci_core_vnic_attachment.fw_lan.create_vnic_details).subnet_id == "ocid1.subnet.oc1..lan" && one(oci_core_vnic_attachment.fw_lan.create_vnic_details).private_ip == "10.70.20.254" && oci_core_vnic_attachment.fw_lan.instance_id == "ocid1.instance.oc1..fw"
    error_message = "fw must have a VNIC on lan at 10.70.20.254"
  }
  assert {
    condition     = one(oci_core_instance.fw.create_vnic_details).skip_source_dest_check && one(oci_core_vnic_attachment.fw_dmz.create_vnic_details).skip_source_dest_check && one(oci_core_vnic_attachment.fw_lan.create_vnic_details).skip_source_dest_check
    error_message = "fw's VNICs must skip the source and destination check, or Oracle Cloud drops what it forwards"
  }
  assert {
    condition     = tostring(one(oci_core_vnic_attachment.fw_dmz.create_vnic_details).assign_public_ip) == "false" && tostring(one(oci_core_vnic_attachment.fw_lan.create_vnic_details).assign_public_ip) == "false"
    error_message = "fw's dmz and lan VNICs must have no public address"
  }
}

run "each_network_admits_itself_ssh_and_its_reach_rules_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.outside.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.70.0.0/24",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "outside must admit outside, SSH from allowed_cidr and ICMP's fragmentation-needed only"
  }
  assert {
    condition = sort([for r in oci_core_security_list.dmz.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.70.10.0/24",
      "6 10.70.0.0/24 80-80",
      "17 10.70.0.0/24 80-80",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "dmz must admit dmz, outside on 80 (reach), SSH from allowed_cidr and ICMP's fragmentation-needed, and nothing else"
  }
  assert {
    condition = sort([for r in oci_core_security_list.lan.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.70.20.0/24",
      "6 10.70.10.0/24 6379-6379",
      "17 10.70.10.0/24 6379-6379",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "lan must admit lan, dmz on 6379 (reach), SSH from allowed_cidr and ICMP's fragmentation-needed, and nothing else"
  }
  assert {
    condition = sort([for r in oci_core_security_list.outside.egress_security_rules : join(" ", concat([r.protocol, r.destination], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"])) if startswith(r.destination, "10.")]) == sort([
      "all 10.70.0.0/24",
      "6 10.70.10.0/24 80-80",
      "17 10.70.10.0/24 80-80",
    ])
    error_message = "outside's machines must go out to outside, and to dmz on 80 (reach), and to no other network"
  }
}

run "the_outputs_name_the_way_in_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
  assert {
    condition     = keys(output.machines) == ["cache", "fw", "user", "web"] && output.ssh_users == { fw = "ubuntu", web = "ubuntu", cache = "ubuntu", user = "ubuntu" }
    error_message = "`machines` and `ssh_users` must name every machine"
  }
}

# The spec: lan has `internet: false` and `gateway: fw`, so what leaves lan is fw's to decide. On
# Oracle Cloud lan's own table still sends 0.0.0.0/0 straight to the internet gateway, and cache
# has a public address (it is set up over SSH there), so lan's traffic to the internet never
# crosses fw. (lan's security list lets its machines out to every public block, which a route
# through fw needs: the list judges the destination, not the next hop.) Only the public address
# is asserted: a failing assertion that reads a route table's `route_rules` crashes Terraform
# 1.16's JSON view (the rules carry a deprecated attribute's mark).
run "known_bug_a_routed_network_without_internet_goes_out_around_its_gateway" {
  command = plan

  assert {
    condition     = tostring(one(oci_core_instance.cache.create_vnic_details).assign_public_ip) == "false"
    error_message = "cache, on lan (internet: false, routed by fw), must have no public address: its traffic to the internet must cross fw"
  }
}
