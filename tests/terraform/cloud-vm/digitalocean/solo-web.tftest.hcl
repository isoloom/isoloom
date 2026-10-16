# examples/solo-web on DigitalOcean (cloud-vm): one droplet in a VPC with the network's range,
# its web port (80, http) published as 8080. DigitalOcean picks the droplet's private address
# itself, so the spec's address (10.80.0.10) is best effort there, as the module says.

mock_provider "digitalocean" {
  override_during = plan
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
  target          = digitalocean_vpc.env
  override_during = plan
  values          = { id = "vpc-env" }
}

override_resource {
  target          = digitalocean_droplet.web
  override_during = plan
  values          = { id = "4242", ipv4_address = "198.51.100.10" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_droplet_is_in_the_network_s_vpc" {
  command = plan

  assert {
    condition     = digitalocean_vpc.env.ip_range == "10.80.0.0/24"
    error_message = "the VPC must hold app's exact range"
  }
  assert {
    condition     = digitalocean_droplet.web.vpc_uuid == "vpc-env"
    error_message = "web must be in the environment's VPC"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_only" {
  command = plan

  assert {
    condition     = tolist(digitalocean_firewall.web.droplet_ids) == tolist([4242])
    error_message = "the firewall must apply to web"
  }
  assert {
    condition = sort([for r in digitalocean_firewall.web.inbound_rule : "${r.protocol} ${coalesce(r.port_range, "-")} ${join(",", r.source_addresses)}"]) == sort([
      "tcp 22 203.0.113.7/32",
      "tcp 8080 203.0.113.7/32",
      "tcp 1-65535 10.80.0.0/24",
      "udp 1-65535 10.80.0.0/24",
      "icmp - 10.80.0.0/24",
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
