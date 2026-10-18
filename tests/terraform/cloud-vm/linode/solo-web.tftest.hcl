# examples/solo-web on Linode (cloud-vm): one machine on a VPC subnet with the network's range,
# its web port (80, http) published as 8080. Linode's firewall drops what it doesn't accept.

mock_provider "linode" {
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
  target          = linode_vpc_subnet.app
  override_during = plan
  values          = { id = 7 }
}

override_resource {
  target          = linode_instance.web
  override_during = plan
  values          = { id = "4242", ipv4 = ["198.51.100.10"] }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
}

run "the_machine_is_on_its_network_at_its_address" {
  command = plan

  assert {
    condition     = linode_vpc_subnet.app.ipv4 == "10.80.0.0/24"
    error_message = "app's subnet must hold the network's exact range"
  }
  assert {
    condition     = [for i in linode_instance.web.interface : "${i.purpose} ${coalesce(i.subnet_id, 0)} ${join(",", [for a in coalesce(i.ipv4, []) : coalesce(a.vpc, "-")])}"] == ["public 0 ", "vpc 7 10.80.0.10"]
    error_message = "web must have its public interface (Terraform sets it up there) and one on app at 10.80.0.10"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_only" {
  command = plan

  assert {
    condition     = linode_firewall.web.inbound_policy == "DROP" && tolist(linode_firewall.web.linodes) == tolist([4242])
    error_message = "web's firewall must drop what it doesn't accept, and apply to web"
  }
  assert {
    condition = sort([for r in linode_firewall.web.inbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 22 203.0.113.7/32",
      "ACCEPT TCP 8080 203.0.113.7/32",
      "ACCEPT TCP 1-65535 10.80.0.0/24",
      "ACCEPT UDP 1-65535 10.80.0.0/24",
      "ACCEPT ICMP - 10.80.0.0/24",
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
