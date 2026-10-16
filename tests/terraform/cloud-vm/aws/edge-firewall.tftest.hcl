# examples/edge-firewall on AWS (cloud-vm): `fw` routes dmz and lan (`gateway: fw`), so traffic
# between the networks crosses it; `reach` opens outside to dmz on 80 and dmz to lan on 6379;
# lan has no internet. AWS keeps a subnet's first address, so fw takes .254 on the networks it
# routes. Every run plans against mocked providers; the ids the assertions compare are pinned.

mock_provider "aws" {
  override_during = plan
  source          = "./tests/mocks/aws"
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
  target          = aws_internet_gateway.env
  override_during = plan
  values          = { id = "igw-env" }
}

override_resource {
  target          = aws_subnet.outside
  override_during = plan
  values          = { id = "subnet-outside" }
}

override_resource {
  target          = aws_subnet.dmz
  override_during = plan
  values          = { id = "subnet-dmz" }
}

override_resource {
  target          = aws_subnet.lan
  override_during = plan
  values          = { id = "subnet-lan" }
}

override_resource {
  target          = aws_route_table.outside
  override_during = plan
  values          = { id = "rtb-outside" }
}

override_resource {
  target          = aws_route_table.dmz
  override_during = plan
  values          = { id = "rtb-dmz" }
}

override_resource {
  target          = aws_route_table.lan
  override_during = plan
  values          = { id = "rtb-lan" }
}

override_resource {
  target          = aws_network_interface.fw_outside
  override_during = plan
  values          = { id = "eni-fw-outside" }
}

override_resource {
  target          = aws_network_interface.fw_dmz
  override_during = plan
  values          = { id = "eni-fw-dmz" }
}

override_resource {
  target          = aws_network_interface.fw_lan
  override_during = plan
  values          = { id = "eni-fw-lan" }
}

override_resource {
  target          = aws_security_group.fw
  override_during = plan
  values          = { id = "sg-fw" }
}

override_resource {
  target          = aws_instance.fw
  override_during = plan
  values          = { id = "i-fw" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "every_network_holds_its_range_inside_the_vpc_block" {
  command = plan

  assert {
    condition     = aws_vpc.env.cidr_block == "10.70.0.0/19"
    error_message = "the VPC's block must be the smallest one holding every network, so a route through fw is more specific than it"
  }
  assert {
    condition     = [aws_subnet.outside.cidr_block, aws_subnet.dmz.cidr_block, aws_subnet.lan.cidr_block] == ["10.70.0.0/24", "10.70.10.0/24", "10.70.20.0/24"]
    error_message = "each network's subnet must hold the network's exact range"
  }
}

run "each_network_has_its_table_sending_the_others_through_fw" {
  command = plan

  assert {
    condition     = aws_route_table_association.outside.route_table_id == "rtb-outside" && aws_route_table_association.dmz.route_table_id == "rtb-dmz" && aws_route_table_association.lan.route_table_id == "rtb-lan"
    error_message = "each network's subnet must use its own route table"
  }
  assert {
    condition     = sort([for r in aws_route_table.outside.route : "${r.cidr_block} ${coalesce(r.gateway_id, r.network_interface_id)}" if r.cidr_block != "0.0.0.0/0"]) == sort(["10.70.10.0/24 eni-fw-outside", "10.70.20.0/24 eni-fw-outside"])
    error_message = "outside must reach dmz and lan through fw's interface on outside"
  }
  assert {
    condition     = sort([for r in aws_route_table.dmz.route : "${r.cidr_block} ${coalesce(r.gateway_id, r.network_interface_id)}" if r.cidr_block != "0.0.0.0/0"]) == sort(["10.70.0.0/24 eni-fw-dmz", "10.70.20.0/24 eni-fw-dmz"])
    error_message = "dmz (routed by fw) must reach every other network through fw's interface on dmz"
  }
  assert {
    condition     = sort([for r in aws_route_table.lan.route : "${r.cidr_block} ${coalesce(r.gateway_id, r.network_interface_id)}" if r.cidr_block != "0.0.0.0/0"]) == sort(["10.70.0.0/24 eni-fw-lan", "10.70.10.0/24 eni-fw-lan"])
    error_message = "lan (routed by fw) must reach every other network through fw's interface on lan"
  }
  assert {
    condition     = [for r in aws_route_table.outside.route : r.gateway_id if r.cidr_block == "0.0.0.0/0"] == ["igw-env"] && [for r in aws_route_table.dmz.route : r.gateway_id if r.cidr_block == "0.0.0.0/0"] == ["igw-env"]
    error_message = "outside and dmz (internet: true) must route the internet through the internet gateway"
  }
}

run "fw_forwards_on_an_interface_per_network" {
  command = plan

  assert {
    condition     = [aws_network_interface.fw_outside.subnet_id, aws_network_interface.fw_dmz.subnet_id, aws_network_interface.fw_lan.subnet_id] == ["subnet-outside", "subnet-dmz", "subnet-lan"]
    error_message = "fw must have one interface on each of its networks"
  }
  assert {
    condition     = [tolist(aws_network_interface.fw_outside.private_ips), tolist(aws_network_interface.fw_dmz.private_ips), tolist(aws_network_interface.fw_lan.private_ips)] == [tolist(["10.70.0.5"]), tolist(["10.70.10.254"]), tolist(["10.70.20.254"])]
    error_message = "fw must be at 10.70.0.5 on outside, and at .254 on dmz and lan (AWS keeps the gateway address .1)"
  }
  assert {
    condition     = !aws_network_interface.fw_outside.source_dest_check && !aws_network_interface.fw_dmz.source_dest_check && !aws_network_interface.fw_lan.source_dest_check
    error_message = "fw's interfaces must not check source and destination, or AWS drops what it forwards"
  }
  assert {
    condition     = [for n in aws_instance.fw.primary_network_interface : n.network_interface_id] == ["eni-fw-outside"] && aws_network_interface_attachment.fw_dmz.network_interface_id == "eni-fw-dmz" && aws_network_interface_attachment.fw_lan.network_interface_id == "eni-fw-lan"
    error_message = "fw's first interface must be on outside, the other two attached to it"
  }
  assert {
    condition     = aws_network_interface_attachment.fw_dmz.instance_id == "i-fw" && aws_network_interface_attachment.fw_lan.instance_id == "i-fw" && aws_eip.fw.network_interface == "eni-fw-outside"
    error_message = "fw's dmz and lan interfaces must be attached to fw, and its public address on its outside interface"
  }
}

run "each_machine_admits_its_networks_ssh_and_its_reach_rules_only" {
  command = plan

  assert {
    condition     = tolist(aws_network_interface.fw_dmz.security_groups) == tolist(["sg-fw"]) && tolist(aws_network_interface.fw_lan.security_groups) == tolist(["sg-fw"]) && tolist(aws_network_interface.fw_outside.security_groups) == tolist(["sg-fw"])
    error_message = "fw's interfaces must all be behind fw's security group"
  }
  assert {
    condition = sort([for r in aws_security_group.fw.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.70.0.0/24",
      "-1 0-0 10.70.10.0/24",
      "-1 0-0 10.70.20.0/24",
      "tcp 22-22 203.0.113.7/32",
    ])
    error_message = "fw routes: it must admit its three networks and SSH from allowed_cidr, and nothing else"
  }
  assert {
    condition = sort([for r in aws_security_group.web.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.70.10.0/24",
      "tcp 22-22 203.0.113.7/32",
      "tcp 80-80 10.70.0.0/24",
      "udp 80-80 10.70.0.0/24",
    ])
    error_message = "web (on dmz) must admit dmz, SSH, and outside on 80 (reach), and nothing else"
  }
  assert {
    condition = sort([for r in aws_security_group.cache.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.70.20.0/24",
      "tcp 22-22 203.0.113.7/32",
      "tcp 6379-6379 10.70.10.0/24",
      "udp 6379-6379 10.70.10.0/24",
    ])
    error_message = "cache (on lan) must admit lan, SSH, and dmz on 6379 (reach), and nothing else"
  }
  assert {
    condition = sort([for r in aws_security_group.user.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.70.0.0/24",
      "tcp 22-22 203.0.113.7/32",
    ])
    error_message = "user (on outside) must admit outside and SSH from allowed_cidr only"
  }
}

# The spec: lan has `internet: false` and `gateway: fw`, so what leaves lan is fw's to decide. On
# AWS lan's table still sends 0.0.0.0/0 straight to the internet gateway and cache has a public
# address (the machines are set up over SSH there), so lan's traffic to the internet never
# crosses fw, and nothing else stops it.
run "known_bug_a_routed_network_without_internet_goes_out_around_its_gateway" {
  command = plan

  assert {
    condition     = length([for r in aws_route_table.lan.route : r if r.cidr_block == "0.0.0.0/0" && r.gateway_id == "igw-env"]) == 0
    error_message = "lan (internet: false, routed by fw) must have no route to the internet gateway"
  }
  assert {
    condition     = aws_instance.cache.associate_public_ip_address == false
    error_message = "cache, on lan (internet: false), must have no public address"
  }
}
