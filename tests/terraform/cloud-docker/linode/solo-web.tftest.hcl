# examples/solo-web on Linode (cloud-docker): one host VM runs the docker target's Compose
# file, so the lab's networks live inside Compose; the cloud's part is an instance and its firewall: SSH and the published port 8080/tcp (web:80) from allowed_cidr. Every run plans against mocked providers;
# the ids the assertions compare are pinned below.

mock_provider "linode" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = linode_instance.env
  override_during = plan
  values          = { id = "4242", ipv4 = ["198.51.100.10"] }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
}

run "the_host_admits_ssh_and_the_published_port_from_allowed_cidr_only" {
  command = plan

  assert {
    condition     = linode_firewall.env.inbound_policy == "DROP" && tolist(linode_firewall.env.linodes) == tolist([4242])
    error_message = "the host's firewall must drop what it doesn't accept, and apply to the host"
  }
  assert {
    condition = sort([for r in linode_firewall.env.inbound : "${r.action} ${r.protocol} ${coalesce(r.ports, "-")} ${join(",", r.ipv4)}"]) == sort([
      "ACCEPT TCP 22,8080 203.0.113.7/32",
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
    condition     = output.ssh_user == "root"
    error_message = "`ssh_user` must be the host image's user"
  }
}

