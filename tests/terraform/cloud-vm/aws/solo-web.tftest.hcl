# examples/solo-web on AWS (cloud-vm): one machine on one network, its web port (80, http)
# published as 8080. Every run plans against mocked providers; the ids and the address the
# assertions compare are pinned below.

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
  target          = aws_subnet.app
  override_during = plan
  values          = { id = "subnet-app" }
}

override_resource {
  target          = aws_security_group.web
  override_during = plan
  values          = { id = "sg-web" }
}

override_resource {
  target          = aws_instance.web
  override_during = plan
  values          = { public_ip = "198.51.100.10" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "the_network_reaches_the_internet_through_the_gateway" {
  command = plan

  assert {
    condition     = aws_vpc.env.cidr_block == "10.80.0.0/24" && aws_subnet.app.cidr_block == "10.80.0.0/24" && aws_subnet.app.vpc_id == "vpc-env"
    error_message = "app's subnet must hold the network's exact range, in the environment's VPC"
  }
  assert {
    condition     = [for r in aws_route_table.env.route : "${r.cidr_block} ${r.gateway_id}"] == ["0.0.0.0/0 igw-env"]
    error_message = "app (internet: true) must route the internet through the internet gateway, and nothing else"
  }
  assert {
    condition     = aws_route_table_association.app.subnet_id == "subnet-app" && aws_route_table_association.app.route_table_id == "rtb-env"
    error_message = "app's subnet must use the environment's route table"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_only" {
  command = plan

  assert {
    condition     = aws_instance.web.subnet_id == "subnet-app" && aws_instance.web.private_ip == "10.80.0.10" && tolist(aws_instance.web.vpc_security_group_ids) == tolist(["sg-web"])
    error_message = "web must sit on app at 10.80.0.10, behind its own security group only"
  }
  assert {
    condition = sort([for r in aws_security_group.web.ingress : "${r.protocol} ${r.from_port}-${r.to_port} ${join(",", r.cidr_blocks)}"]) == sort([
      "-1 0-0 10.80.0.0/24",
      "tcp 22-22 203.0.113.7/32",
      "tcp 8080-8080 203.0.113.7/32",
    ])
    error_message = "web must admit its network, SSH and its published port (8080/tcp) from allowed_cidr, and nothing else"
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
