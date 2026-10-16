# examples/segmented on Linode (cloud-vm): three VPC subnets; Linode routes between a VPC's
# subnets itself, so each machine's firewall carries `reach` both ways: inbound, its network,
# SSH and the reach rules into it; outbound, the reach rules out of it, then a drop toward every
# other network. `back` has no internet.

mock_provider "linode" {
  override_during = plan
}

mock_provider "local" {
  override_during = plan
}

mock_provider "random" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = linode_vpc_subnet.front
  override_during = plan
  values          = { id = 1 }
}

override_resource {
  target          = linode_vpc_subnet.back
  override_during = plan
  values          = { id = 2 }
}

override_resource {
  target          = linode_vpc_subnet.access
  override_during = plan
  values          = { id = 3 }
}

override_resource {
  target          = linode_instance.cache
  override_during = plan
  values          = { id = "4241", ipv4 = ["198.51.100.20"] }
}

override_resource {
  target          = linode_instance.web
  override_during = plan
  values          = { id = "4242", ipv4 = ["198.51.100.10"] }
}

override_resource {
  target          = linode_instance.user
  override_during = plan
  values          = { id = "4243", ipv4 = ["198.51.100.30"] }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
}

run "each_network_is_a_subnet_of_the_vpc_at_its_exact_range" {
  command = plan

  assert {
    condition     = [linode_vpc_subnet.front.ipv4, linode_vpc_subnet.back.ipv4, linode_vpc_subnet.access.ipv4] == ["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"]
    error_message = "each network's subnet must hold the network's exact range"
  }
}

run "each_machine_is_on_its_network_at_its_address" {
  command = plan

  assert {
    condition     = [for i in linode_instance.cache.interface : "${i.purpose} ${coalesce(i.subnet_id, 0)} ${join(",", [for a in coalesce(i.ipv4, []) : coalesce(a.vpc, "-")])}"] == ["public 0 ", "vpc 2 10.61.20.20"]
    error_message = "cache must be on back at 10.61.20.20 (and its public interface)"
  }
  assert {
    condition     = [for i in linode_instance.web.interface : "${i.purpose} ${coalesce(i.subnet_id, 0)} ${join(",", [for a in coalesce(i.ipv4, []) : coalesce(a.vpc, "-")])}"] == ["public 0 ", "vpc 1 10.61.10.10"]
    error_message = "web must be on front at 10.61.10.10 (and its public interface)"
  }
  assert {
    condition     = [for i in linode_instance.user.interface : "${i.purpose} ${coalesce(i.subnet_id, 0)} ${join(",", [for a in coalesce(i.ipv4, []) : coalesce(a.vpc, "-")])}"] == ["public 0 ", "vpc 3 10.61.99.10"]
    error_message = "user must be on access at 10.61.99.10 (and its public interface)"
  }
  assert {
    condition     = tolist(linode_firewall.cache.linodes) == tolist([4241]) && tolist(linode_firewall.web.linodes) == tolist([4242]) && tolist(linode_firewall.user.linodes) == tolist([4243])
    error_message = "each machine must be behind its own firewall"
  }
}

run "the_cache_admits_back_and_front_on_6379_and_reaches_no_other_network" {
  command = plan

  assert {
    condition     = linode_firewall.cache.inbound_policy == "DROP" && linode_firewall.cache.outbound_policy == "ACCEPT"
    error_message = "cache's firewall must drop what it doesn't accept inbound"
  }
  assert {
    condition = sort([for r in linode_firewall.cache.inbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 22 203.0.113.7/32",
      "ACCEPT TCP 1-65535 10.61.20.0/24",
      "ACCEPT UDP 1-65535 10.61.20.0/24",
      "ACCEPT ICMP - 10.61.20.0/24",
      "ACCEPT TCP 6379 10.61.10.0/24",
      "ACCEPT UDP 6379 10.61.10.0/24",
    ])
    error_message = "cache (on back) must admit back, SSH from allowed_cidr and front on 6379 (reach), and nothing else"
  }
  assert {
    condition = sort([for r in linode_firewall.cache.outbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "DROP TCP 1-65535 10.61.10.0/24",
      "DROP UDP 1-65535 10.61.10.0/24",
      "DROP ICMP - 10.61.10.0/24",
      "DROP TCP 1-65535 10.61.99.0/24",
      "DROP UDP 1-65535 10.61.99.0/24",
      "DROP ICMP - 10.61.99.0/24",
    ])
    error_message = "cache must not open connections to front or access (no reach rule out of back)"
  }
}

run "the_web_server_admits_front_and_access_and_reaches_back_on_6379_only" {
  command = plan

  assert {
    condition     = linode_firewall.web.inbound_policy == "DROP" && linode_firewall.web.outbound_policy == "ACCEPT"
    error_message = "web's firewall must drop what it doesn't accept inbound"
  }
  assert {
    condition = sort([for r in linode_firewall.web.inbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 22 203.0.113.7/32",
      "ACCEPT TCP 1-65535 10.61.10.0/24",
      "ACCEPT UDP 1-65535 10.61.10.0/24",
      "ACCEPT ICMP - 10.61.10.0/24",
      "ACCEPT TCP 1-65535 10.61.99.0/24",
      "ACCEPT UDP 1-65535 10.61.99.0/24",
      "ACCEPT ICMP - 10.61.99.0/24",
    ])
    error_message = "web (on front) must admit front, SSH from allowed_cidr and access on every port (reach), and nothing else"
  }
  assert {
    condition = sort([for r in linode_firewall.web.outbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 6379 10.61.20.0/24",
      "ACCEPT UDP 6379 10.61.20.0/24",
      "DROP TCP 1-65535 10.61.20.0/24",
      "DROP UDP 1-65535 10.61.20.0/24",
      "DROP ICMP - 10.61.20.0/24",
      "DROP TCP 1-65535 10.61.99.0/24",
      "DROP UDP 1-65535 10.61.99.0/24",
      "DROP ICMP - 10.61.99.0/24",
    ])
    error_message = "web may reach back on 6379 only (reach), and access not at all"
  }
}

run "the_user_s_machine_admits_access_and_reaches_front_only" {
  command = plan

  assert {
    condition     = linode_firewall.user.inbound_policy == "DROP" && linode_firewall.user.outbound_policy == "ACCEPT"
    error_message = "user's firewall must drop what it doesn't accept inbound"
  }
  assert {
    condition = sort([for r in linode_firewall.user.inbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 22 203.0.113.7/32",
      "ACCEPT TCP 1-65535 10.61.99.0/24",
      "ACCEPT UDP 1-65535 10.61.99.0/24",
      "ACCEPT ICMP - 10.61.99.0/24",
    ])
    error_message = "user (on access) must admit access and SSH from allowed_cidr only"
  }
  assert {
    condition = sort([for r in linode_firewall.user.outbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 1-65535 10.61.10.0/24",
      "ACCEPT UDP 1-65535 10.61.10.0/24",
      "ACCEPT ICMP - 10.61.10.0/24",
      "DROP TCP 1-65535 10.61.10.0/24",
      "DROP UDP 1-65535 10.61.10.0/24",
      "DROP ICMP - 10.61.10.0/24",
      "DROP TCP 1-65535 10.61.20.0/24",
      "DROP UDP 1-65535 10.61.20.0/24",
      "DROP ICMP - 10.61.20.0/24",
    ])
    error_message = "user may reach front (reach), and back not at all"
  }
}

run "the_outputs_name_the_way_in_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.30" && output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ip` must be the access machine's (user's) public address, `ready_file` the isoloom-ready/1 file"
  }
  assert {
    condition     = output.machines == { cache = "198.51.100.20", web = "198.51.100.10", user = "198.51.100.30" }
    error_message = "`machines` must give every machine's public address"
  }
}

# The spec: `back` has `internet: false`, so its machines may not reach the internet. On Linode
# cache has a public interface (Terraform sets it up over SSH there) and its firewall accepts
# everything outbound but the other networks: only the guest's own nftables rule
# (`isoloom-egress`) stops it, which root on the machine can lift.
run "known_bug_a_network_without_internet_still_has_a_way_out" {
  command = plan

  assert {
    condition     = linode_firewall.cache.outbound_policy == "DROP"
    error_message = "cache's firewall, on back (internet: false), must drop outbound traffic it doesn't accept"
  }
}
