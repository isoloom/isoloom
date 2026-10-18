# examples/segmented on AWS (cloud-vm): three networks in one VPC, `reach` from access to front
# (every port) and from front to back (6379 only), `back` without internet. Every run plans
# against mocked providers; the ids the assertions compare are pinned below.

mock_provider "aws" {
  override_during = plan
  source          = "./tests/mocks/aws"
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = aws_vpc.env
  override_during = plan
  values          = { id = "vpc-env" }
}

override_resource {
  target          = aws_internet_gateway.env
  override_during = plan
  values          = { id = "igw-env" }
}

override_resource {
  target          = aws_route_table.env
  override_during = plan
  values          = { id = "rtb-env" }
}

override_resource {
  target          = aws_route_table.sealed
  override_during = plan
  values          = { id = "rtb-sealed" }
}

override_resource {
  target          = aws_subnet.front
  override_during = plan
  values          = { id = "subnet-front" }
}

override_resource {
  target          = aws_subnet.back
  override_during = plan
  values          = { id = "subnet-back" }
}

override_resource {
  target          = aws_subnet.access
  override_during = plan
  values          = { id = "subnet-access" }
}

override_resource {
  target          = aws_security_group.cache
  override_during = plan
  values          = { id = "sg-cache" }
}

override_resource {
  target          = aws_security_group.web
  override_during = plan
  values          = { id = "sg-web" }
}

override_resource {
  target          = aws_security_group.user
  override_during = plan
  values          = { id = "sg-user" }
}

override_resource {
  target          = aws_instance.user
  override_during = plan
  values          = { public_ip = "198.51.100.30" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "each_network_is_a_subnet_of_the_vpc_at_its_exact_range" {
  command = plan

  assert {
    condition     = aws_subnet.front.cidr_block == "10.61.10.0/24" && aws_subnet.back.cidr_block == "10.61.20.0/24" && aws_subnet.access.cidr_block == "10.61.99.0/24"
    error_message = "each network's subnet must have the network's exact range"
  }
  assert {
    condition     = alltrue([for s in [aws_subnet.front, aws_subnet.back, aws_subnet.access] : s.vpc_id == "vpc-env"])
    error_message = "every network's subnet must be in the environment's one VPC"
  }
  assert {
    condition = toset(concat([aws_vpc.env.cidr_block], [aws_vpc_ipv4_cidr_block_association.back.cidr_block, aws_vpc_ipv4_cidr_block_association.access.cidr_block])) == toset(["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"])
    error_message = "the VPC's blocks must be exactly the three networks'"
  }
}

run "the_internet_gateway_is_the_only_way_out" {
  command = plan

  assert {
    condition     = aws_internet_gateway.env.vpc_id == "vpc-env"
    error_message = "the internet gateway must be attached to the environment's VPC (the machines are set up over their public addresses)"
  }
  assert {
    condition     = [for r in aws_route_table.env.route : { cidr = r.cidr_block, gateway = r.gateway_id }] == [{ cidr = "0.0.0.0/0", gateway = "igw-env" }]
    error_message = "the environment's route table must hold one route, the default one through the internet gateway"
  }
  assert {
    condition     = [aws_route_table_association.front.subnet_id, aws_route_table_association.front.route_table_id] == ["subnet-front", "rtb-env"]
    error_message = "front's subnet must use the environment's route table"
  }
  assert {
    condition     = [aws_route_table_association.back.subnet_id, aws_route_table_association.back.route_table_id] == ["subnet-back", "rtb-sealed"]
    error_message = "back (internet: false) must use the sealed route table, so its way out can be closed once its machines are set up"
  }
  assert {
    condition     = [for r in aws_route_table.sealed.route : { cidr = r.cidr_block, gateway = r.gateway_id }] == [{ cidr = "0.0.0.0/0", gateway = "igw-env" }]
    error_message = "until the run is sealed, back reaches the internet (its machines set themselves up over SSH)"
  }
  assert {
    condition     = [aws_route_table_association.access.subnet_id, aws_route_table_association.access.route_table_id] == ["subnet-access", "rtb-env"]
    error_message = "access's subnet must use the environment's route table"
  }
}

run "each_machine_is_on_its_network_at_its_address_behind_its_own_group" {
  command = plan

  assert {
    condition     = aws_instance.cache.subnet_id == "subnet-back" && aws_instance.cache.private_ip == "10.61.20.20" && tolist(aws_instance.cache.vpc_security_group_ids) == tolist(["sg-cache"])
    error_message = "cache must sit on back at 10.61.20.20, behind its own security group only"
  }
  assert {
    condition     = aws_instance.web.subnet_id == "subnet-front" && aws_instance.web.private_ip == "10.61.10.10" && tolist(aws_instance.web.vpc_security_group_ids) == tolist(["sg-web"])
    error_message = "web must sit on front at 10.61.10.10, behind its own security group only"
  }
  assert {
    condition     = aws_instance.user.subnet_id == "subnet-access" && aws_instance.user.private_ip == "10.61.99.10" && tolist(aws_instance.user.vpc_security_group_ids) == tolist(["sg-user"])
    error_message = "user must sit on access at 10.61.99.10, behind its own security group only"
  }
}

run "the_cache_admits_its_network_ssh_and_front_on_6379_only" {
  command = plan

  assert {
    condition = sort([for r in aws_security_group.cache.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.61.20.0/24",
      "tcp 22-22 203.0.113.7/32",
      "tcp 6379-6379 10.61.10.0/24",
      "udp 6379-6379 10.61.10.0/24",
    ])
    error_message = "cache (on back) must admit back, SSH from allowed_cidr and front on 6379 (reach), and nothing else"
  }
}

run "the_web_server_admits_its_network_ssh_and_access_on_every_port" {
  command = plan

  assert {
    condition = sort([for r in aws_security_group.web.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.61.10.0/24",
      "tcp 22-22 203.0.113.7/32",
      "-1 0-0 10.61.99.0/24",
    ])
    error_message = "web (on front) must admit front, SSH from allowed_cidr and access on every port (reach), and nothing else: no rule from back"
  }
}

run "the_user_s_machine_admits_its_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in aws_security_group.user.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.61.99.0/24",
      "tcp 22-22 203.0.113.7/32",
    ])
    error_message = "user (on access) must admit access and SSH from allowed_cidr only: no reach rule leads into access"
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
    condition     = keys(output.machines) == ["cache", "user", "web"] && output.ssh_users == { cache = "admin", web = "admin", user = "admin" }
    error_message = "`machines` and `ssh_users` must name every machine"
  }
}

# The spec: `back` has `internet: false`, so once its machines are set up they may not reach the
# internet. The runner applies the module again with `isoloom_sealed = true` (the manifest's
# `seal`) once every machine is ready; back's own route table then drops the default route and
# keeps only a route to allowed_cidr, so the runner's SSH and the published ports' replies still
# get out, but the internet does not — at the network, where root in the guest can't lift it. The
# machines keep their public addresses (the seal is egress-only), the way the runner reaches them.
run "a_network_without_internet_is_sealed_at_the_network_after_set_up" {
  command = plan

  variables {
    isoloom_sealed = true
  }

  assert {
    condition     = [for r in aws_route_table.sealed.route : { cidr = r.cidr_block, gateway = r.gateway_id }] == [{ cidr = "203.0.113.7/32", gateway = "igw-env" }]
    error_message = "once sealed, back's only way out must be to allowed_cidr through the internet gateway, never 0.0.0.0/0"
  }
  assert {
    condition     = length([for r in aws_route_table.sealed.route : r if r.cidr_block == "0.0.0.0/0"]) == 0
    error_message = "once sealed, back must have no default route to the internet"
  }
}
