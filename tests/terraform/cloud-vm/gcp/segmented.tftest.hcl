# examples/segmented on Google Cloud (cloud-vm): a VPC network per network, peered with one
# another, `reach` from access to front (every port) and from front to back (6379 only), `back`
# without internet. A machine's firewall rules target its tag: the suite names every rule the
# module writes for each machine, so one more would be a rule nobody asked for. The environment
# uses an existing project (`project`). Every run plans against mocked providers; the ids the
# assertions compare are pinned below, in Google's forms (the provider checks some while
# planning).

mock_provider "google" {
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
  target          = google_compute_network.front
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/front"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/front"
  }
}

override_resource {
  target          = google_compute_subnetwork.front
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/front" }
}

override_resource {
  target          = google_compute_network.back
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/back"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/back"
  }
}

override_resource {
  target          = google_compute_subnetwork.back
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/back" }
}

override_resource {
  target          = google_compute_network.access
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/access"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/access"
  }
}

override_resource {
  target          = google_compute_subnetwork.access
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/access" }
}

override_resource {
  target          = google_compute_instance.user
  override_during = plan
  values = {
    # A list of blocks takes one object, which every element gets: user has one interface.
    network_interface = {
      access_config = { nat_ip = "198.51.100.30" }
    }
  }
}

variables {
  project              = "tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "each_network_is_a_vpc_network_of_its_own_at_its_exact_range" {
  command = plan

  assert {
    condition     = [google_compute_subnetwork.front.ip_cidr_range, google_compute_subnetwork.back.ip_cidr_range, google_compute_subnetwork.access.ip_cidr_range] == ["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"]
    error_message = "each network's subnetwork must hold the network's exact range"
  }
  assert {
    condition     = [google_compute_subnetwork.front.network, google_compute_subnetwork.back.network, google_compute_subnetwork.access.network] == ["projects/tftest/global/networks/front", "projects/tftest/global/networks/back", "projects/tftest/global/networks/access"]
    error_message = "each network's subnetwork must be in its own VPC network (one VPC per network)"
  }
  assert {
    condition     = !google_compute_network.front.auto_create_subnetworks && !google_compute_network.back.auto_create_subnetworks && !google_compute_network.access.auto_create_subnetworks
    error_message = "each VPC network must hold only the environment's subnetwork"
  }
}

run "the_networks_are_peered_with_one_another" {
  command = plan

  assert {
    condition = sort([
      "${basename(google_compute_network_peering.front_back.network)}>${basename(google_compute_network_peering.front_back.peer_network)}",
      "${basename(google_compute_network_peering.back_front.network)}>${basename(google_compute_network_peering.back_front.peer_network)}",
      "${basename(google_compute_network_peering.front_access.network)}>${basename(google_compute_network_peering.front_access.peer_network)}",
      "${basename(google_compute_network_peering.access_front.network)}>${basename(google_compute_network_peering.access_front.peer_network)}",
      "${basename(google_compute_network_peering.back_access.network)}>${basename(google_compute_network_peering.back_access.peer_network)}",
      "${basename(google_compute_network_peering.access_back.network)}>${basename(google_compute_network_peering.access_back.peer_network)}",
      ]) == sort(["front>back", "back>front", "front>access", "access>front", "back>access", "access>back"])
    error_message = "every pair of networks must be peered both ways (the firewall rules, not the peerings, say who reaches whom)"
  }
}

run "the_networks_with_internet_keep_their_default_route" {
  command = plan

  assert {
    condition     = !coalesce(google_compute_network.front.delete_default_routes_on_create, false) && !coalesce(google_compute_network.access.delete_default_routes_on_create, false)
    error_message = "front and access (internet: true) must keep their VPC network's default route to the internet"
  }
}

run "each_machine_is_on_its_network_at_its_address_under_its_own_tag" {
  command = plan

  assert {
    condition     = google_compute_instance.cache.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/back" && google_compute_instance.cache.network_interface[0].network_ip == "10.61.20.20" && tolist(google_compute_instance.cache.tags) == tolist(["isoloom-segmented-t3st1d00-cache"])
    error_message = "cache must sit on back at 10.61.20.20, tagged as itself only"
  }
  assert {
    condition     = google_compute_instance.web.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/front" && google_compute_instance.web.network_interface[0].network_ip == "10.61.10.10" && tolist(google_compute_instance.web.tags) == tolist(["isoloom-segmented-t3st1d00-web"])
    error_message = "web must sit on front at 10.61.10.10, tagged as itself only"
  }
  assert {
    condition     = google_compute_instance.user.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/access" && google_compute_instance.user.network_interface[0].network_ip == "10.61.99.10" && tolist(google_compute_instance.user.tags) == tolist(["isoloom-segmented-t3st1d00-user"])
    error_message = "user must sit on access at 10.61.99.10, tagged as itself only"
  }
  assert {
    condition     = length(google_compute_instance.cache.network_interface) == 1 && length(google_compute_instance.web.network_interface) == 1 && length(google_compute_instance.user.network_interface) == 1
    error_message = "each machine must have one interface, on its one network"
  }
}

run "the_cache_admits_its_network_ssh_and_front_on_6379_only" {
  command = plan

  assert {
    condition = sort([
      "${google_compute_firewall.cache_net_back.direction} ${basename(google_compute_firewall.cache_net_back.network)} ${join(",", sort(google_compute_firewall.cache_net_back.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_net_back.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_net_back.target_tags)}",
      "${google_compute_firewall.cache_ssh.direction} ${basename(google_compute_firewall.cache_ssh.network)} ${join(",", sort(google_compute_firewall.cache_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_ssh.target_tags)}",
      "${google_compute_firewall.cache_reach_front.direction} ${basename(google_compute_firewall.cache_reach_front.network)} ${join(",", sort(google_compute_firewall.cache_reach_front.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_reach_front.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_reach_front.target_tags)}",
      ]) == sort([
      "INGRESS back 10.61.20.0/24 all: isoloom-segmented-t3st1d00-cache",
      "INGRESS back 203.0.113.7/32 tcp:22 isoloom-segmented-t3st1d00-cache",
      "INGRESS back 10.61.10.0/24 tcp:6379 udp:6379 isoloom-segmented-t3st1d00-cache",
    ])
    error_message = "cache (on back) must admit back, SSH from allowed_cidr and front on 6379 (reach), and nothing else"
  }
}

run "the_web_server_admits_its_network_ssh_and_access_on_every_port" {
  command = plan

  assert {
    condition = sort([
      "${google_compute_firewall.web_net_front.direction} ${basename(google_compute_firewall.web_net_front.network)} ${join(",", sort(google_compute_firewall.web_net_front.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_net_front.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_net_front.target_tags)}",
      "${google_compute_firewall.web_ssh.direction} ${basename(google_compute_firewall.web_ssh.network)} ${join(",", sort(google_compute_firewall.web_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_ssh.target_tags)}",
      "${google_compute_firewall.web_reach_access.direction} ${basename(google_compute_firewall.web_reach_access.network)} ${join(",", sort(google_compute_firewall.web_reach_access.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_reach_access.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_reach_access.target_tags)}",
      ]) == sort([
      "INGRESS front 10.61.10.0/24 all: isoloom-segmented-t3st1d00-web",
      "INGRESS front 203.0.113.7/32 tcp:22 isoloom-segmented-t3st1d00-web",
      "INGRESS front 10.61.99.0/24 all: isoloom-segmented-t3st1d00-web",
    ])
    error_message = "web (on front) must admit front, SSH from allowed_cidr and access on every port (reach), and nothing else: no rule from back"
  }
}

run "the_user_s_machine_admits_its_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([
      "${google_compute_firewall.user_net_access.direction} ${basename(google_compute_firewall.user_net_access.network)} ${join(",", sort(google_compute_firewall.user_net_access.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.user_net_access.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.user_net_access.target_tags)}",
      "${google_compute_firewall.user_ssh.direction} ${basename(google_compute_firewall.user_ssh.network)} ${join(",", sort(google_compute_firewall.user_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.user_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.user_ssh.target_tags)}",
      ]) == sort([
      "INGRESS access 10.61.99.0/24 all: isoloom-segmented-t3st1d00-user",
      "INGRESS access 203.0.113.7/32 tcp:22 isoloom-segmented-t3st1d00-user",
    ])
    error_message = "user (on access) must admit access and SSH from allowed_cidr only: no reach rule leads into access"
  }
}

run "the_outputs_name_the_machines_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.30"
    error_message = "`ip` must be the access machine's (user's) external address"
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

# The spec: `back` has `internet: false`, so its machines may not reach the internet. On Google
# Cloud back's VPC network keeps the default route to the internet gateway Google creates with
# it (`delete_default_routes_on_create` is unset: false), and cache has an external address
# (`access_config`: it is set up over SSH there): only the guest's own nftables rule
# (`isoloom-egress`) stops it, which root on the machine can lift.
run "known_bug_a_network_without_internet_still_routes_to_it" {
  command = plan

  assert {
    condition     = coalesce(google_compute_network.back.delete_default_routes_on_create, false) == true
    error_message = "back (internet: false) must have no default route to the internet"
  }
  assert {
    condition     = length(google_compute_instance.cache.network_interface[0].access_config) == 0
    error_message = "cache, on back (internet: false), must have no external address"
  }
}
