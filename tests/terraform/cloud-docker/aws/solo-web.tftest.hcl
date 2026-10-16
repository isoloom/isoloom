# examples/solo-web on AWS (cloud-docker): one host VM in a VPC of its own runs the docker
# target's Compose file, so the lab's networks live inside Compose; the cloud's part is the
# host's subnet, its way out, and its security group: SSH and the published port 8080/tcp
# (web:80) from allowed_cidr. Every run plans against mocked providers; the ids the assertions
# compare are pinned below.

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
  target          = aws_subnet.env
  override_during = plan
  values          = { id = "subnet-env" }
}

override_resource {
  target          = aws_security_group.env
  override_during = plan
  values          = { id = "sg-env" }
}

override_resource {
  target          = aws_instance.env
  override_during = plan
  values          = { public_ip = "198.51.100.10" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "the_host_sits_on_the_one_subnet_of_its_vpc_behind_its_group" {
  command = plan

  assert {
    condition     = aws_vpc.env.cidr_block == "10.42.0.0/16" && aws_subnet.env.cidr_block == "10.42.1.0/24" && aws_subnet.env.vpc_id == "vpc-env"
    error_message = "the host's subnet must be 10.42.1.0/24, in the environment's VPC (10.42.0.0/16)"
  }
  assert {
    condition     = aws_instance.env.subnet_id == "subnet-env" && tolist(aws_instance.env.vpc_security_group_ids) == tolist(["sg-env"])
    error_message = "the host must sit on the environment's subnet, behind the environment's security group only"
  }
}

run "the_internet_gateway_is_the_host_s_way_out" {
  command = plan

  assert {
    condition     = aws_internet_gateway.env.vpc_id == "vpc-env"
    error_message = "the internet gateway must be attached to the environment's VPC (the host installs Docker and is set up over its public address)"
  }
  assert {
    condition     = [for r in aws_route_table.env.route : "${r.cidr_block} ${r.gateway_id}"] == ["0.0.0.0/0 igw-env"]
    error_message = "the route table must hold one route, the default one through the internet gateway"
  }
  assert {
    condition     = aws_route_table_association.env.subnet_id == "subnet-env" && aws_route_table_association.env.route_table_id == "rtb-env"
    error_message = "the host's subnet must use the environment's route table"
  }
}

run "the_host_admits_ssh_and_the_published_port_from_allowed_cidr_only" {
  command = plan

  assert {
    condition = sort([for r in aws_security_group.env.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "tcp 22-22 203.0.113.7/32",
      "tcp 8080-8080 203.0.113.7/32",
    ])
    error_message = "the host must admit SSH and web's published port 8080/tcp from allowed_cidr, and nothing else"
  }
}

run "the_outputs_name_the_host_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.10"
    error_message = "`ip` must be the host's public address"
  }
  assert {
    condition     = output.ssh_user == "admin"
    error_message = "`ssh_user` must be the Debian image's user"
  }
}

