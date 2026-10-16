# examples/segmented on Google Cloud (cloud-docker): one host VM runs the docker target's Compose
# file, so the lab's three networks, its `reach` rules and `back` without
# internet live inside Compose; the cloud's part is its network and subnetwork, and the firewall: SSH from allowed_cidr, nothing else, since no port is published. Every run plans against mocked providers;
# the ids the assertions compare are pinned below.

mock_provider "google" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = google_compute_network.env
  override_during = plan
  values          = { id = "projects/p/global/networks/env" }
}

override_resource {
  target          = google_compute_subnetwork.env
  override_during = plan
  values          = { id = "projects/p/regions/r/subnetworks/env" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  project              = "isoloom-test"
}

run "the_host_sits_on_the_subnetwork_of_its_own_network" {
  command = plan

  assert {
    condition     = google_compute_subnetwork.env.ip_cidr_range == "10.42.1.0/24" && google_compute_subnetwork.env.network == "projects/p/global/networks/env"
    error_message = "the host's subnetwork must be 10.42.1.0/24, in the environment's network"
  }
  assert {
    condition     = google_compute_network.env.auto_create_subnetworks == false
    error_message = "the environment's network must hold only its own subnetwork"
  }
  assert {
    condition     = [for n in google_compute_instance.env.network_interface : n.subnetwork] == ["projects/p/regions/r/subnetworks/env"] && length(google_compute_instance.env.network_interface[0].access_config) == 1
    error_message = "the host's one interface must be on that subnetwork, with a public address (its way out: Docker's install, and Terraform's SSH)"
  }
}

run "the_host_admits_ssh_from_allowed_cidr_only" {
  command = plan

  assert {
    condition     = google_compute_firewall.env.network == "projects/p/global/networks/env"
    error_message = "the firewall must be on the environment's network (inbound, its default)"
  }
  assert {
    condition     = tolist(google_compute_firewall.env.source_ranges) == tolist(["203.0.113.7/32"])
    error_message = "the firewall must admit allowed_cidr only"
  }
  assert {
    condition = sort([for a in google_compute_firewall.env.allow : "${a.protocol} ${join(",", a.ports)}"]) == sort([
      "tcp 22",
    ])
    error_message = "the host must admit SSH, and nothing else"
  }
}

# `ip` is the host's public address, which only Google Cloud knows: unknown while planning.
run "the_outputs_name_the_host_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ssh_user == "isoloom"
    error_message = "`ssh_user` must be the host image's user"
  }
}
