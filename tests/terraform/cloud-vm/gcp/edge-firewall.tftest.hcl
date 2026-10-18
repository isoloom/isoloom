# examples/edge-firewall on Google Cloud (cloud-vm): `fw` routes dmz and lan (`gateway: fw`), so
# traffic between the networks crosses it; `reach` opens outside to dmz on 80 and dmz to lan on
# 6379; lan has no internet. Each network is a VPC network of its own; fw has an interface in
# each, and each network's routes send the others to fw's address in it (Google keeps a
# subnetwork's gateway address, so fw takes the highest free one, .252, on the networks it routes). A
# machine's firewall rules target its tag: the suite names every rule the module writes for each
# machine. The environment uses an existing project (`project`). Every run plans against mocked
# providers; the ids the assertions compare are pinned below, in Google's forms.

mock_provider "google" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = google_compute_network.outside
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/outside"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/outside"
  }
}

override_resource {
  target          = google_compute_subnetwork.outside
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/outside" }
}

override_resource {
  target          = google_compute_network.dmz
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/dmz"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/dmz"
  }
}

override_resource {
  target          = google_compute_subnetwork.dmz
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/dmz" }
}

override_resource {
  target          = google_compute_network.lan
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/lan"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/lan"
  }
}

override_resource {
  target          = google_compute_subnetwork.lan
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/lan" }
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
}

run "each_network_is_a_vpc_network_of_its_own_at_its_exact_range" {
  command = plan

  assert {
    condition     = [google_compute_subnetwork.outside.ip_cidr_range, google_compute_subnetwork.dmz.ip_cidr_range, google_compute_subnetwork.lan.ip_cidr_range] == ["10.70.0.0/24", "10.70.10.0/24", "10.70.20.0/24"]
    error_message = "each network's subnetwork must hold the network's exact range"
  }
  assert {
    condition     = [google_compute_subnetwork.outside.network, google_compute_subnetwork.dmz.network, google_compute_subnetwork.lan.network] == ["projects/tftest/global/networks/outside", "projects/tftest/global/networks/dmz", "projects/tftest/global/networks/lan"]
    error_message = "each network's subnetwork must be in its own VPC network (one VPC per network)"
  }
}

run "each_network_sends_the_others_through_fw" {
  command = plan

  assert {
    condition = sort([
      "${basename(google_compute_route.outside_dmz.network)} ${google_compute_route.outside_dmz.dest_range} ${google_compute_route.outside_dmz.next_hop_ip}",
      "${basename(google_compute_route.outside_lan.network)} ${google_compute_route.outside_lan.dest_range} ${google_compute_route.outside_lan.next_hop_ip}",
      "${basename(google_compute_route.dmz_outside.network)} ${google_compute_route.dmz_outside.dest_range} ${google_compute_route.dmz_outside.next_hop_ip}",
      "${basename(google_compute_route.dmz_lan.network)} ${google_compute_route.dmz_lan.dest_range} ${google_compute_route.dmz_lan.next_hop_ip}",
      "${basename(google_compute_route.lan_outside.network)} ${google_compute_route.lan_outside.dest_range} ${google_compute_route.lan_outside.next_hop_ip}",
      "${basename(google_compute_route.lan_dmz.network)} ${google_compute_route.lan_dmz.dest_range} ${google_compute_route.lan_dmz.next_hop_ip}",
      ]) == sort([
      "outside 10.70.10.0/24 10.70.0.5",
      "outside 10.70.20.0/24 10.70.0.5",
      "dmz 10.70.0.0/24 10.70.10.252",
      "dmz 10.70.20.0/24 10.70.10.252",
      "lan 10.70.0.0/24 10.70.20.252",
      "lan 10.70.10.0/24 10.70.20.252",
    ])
    error_message = "each network must reach every other one through fw's address in it: 10.70.0.5 on outside, 10.70.10.252 on dmz, 10.70.20.252 on lan"
  }
  assert {
    condition     = !coalesce(google_compute_network.outside.delete_default_routes_on_create, false) && !coalesce(google_compute_network.dmz.delete_default_routes_on_create, false)
    error_message = "outside and dmz (internet: true) must keep their VPC network's default route to the internet"
  }
}

run "fw_forwards_on_an_interface_per_network" {
  command = plan

  assert {
    condition = [for n in google_compute_instance.fw.network_interface : "${basename(n.subnetwork)} ${n.network_ip}"] == ["outside 10.70.0.5", "dmz 10.70.10.252", "lan 10.70.20.252"]
    error_message = "fw must have one interface per network: outside first (its way in), then dmz and lan at the address the routes name"
  }
  assert {
    condition     = google_compute_instance.fw.can_ip_forward == true
    error_message = "fw must be allowed to forward, or Google drops what it routes"
  }
  assert {
    condition     = [for n in google_compute_instance.fw.network_interface : length(n.access_config)] == [1, 0, 0]
    error_message = "fw must have an external address on outside only"
  }
}

run "each_machine_is_on_its_network_at_its_address_under_its_own_tag" {
  command = plan

  assert {
    condition     = google_compute_instance.web.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/dmz" && google_compute_instance.web.network_interface[0].network_ip == "10.70.10.10" && tolist(google_compute_instance.web.tags) == tolist(["isoloom-edge-firewall-t3st1d00-web"])
    error_message = "web must sit on dmz at 10.70.10.10, tagged as itself only"
  }
  assert {
    condition     = google_compute_instance.cache.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/lan" && google_compute_instance.cache.network_interface[0].network_ip == "10.70.20.20" && tolist(google_compute_instance.cache.tags) == tolist(["isoloom-edge-firewall-t3st1d00-cache"])
    error_message = "cache must sit on lan at 10.70.20.20, tagged as itself only"
  }
  assert {
    condition     = google_compute_instance.user.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/outside" && google_compute_instance.user.network_interface[0].network_ip == "10.70.0.10" && tolist(google_compute_instance.user.tags) == tolist(["isoloom-edge-firewall-t3st1d00-user"])
    error_message = "user must sit on outside at 10.70.0.10, tagged as itself only"
  }
}

run "each_machine_admits_its_networks_ssh_and_its_reach_rules_only" {
  command = plan

  assert {
    condition = sort([
      "${google_compute_firewall.fw_net_outside.direction} ${basename(google_compute_firewall.fw_net_outside.network)} ${join(",", sort(google_compute_firewall.fw_net_outside.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.fw_net_outside.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.fw_net_outside.target_tags)}",
      "${google_compute_firewall.fw_net_dmz.direction} ${basename(google_compute_firewall.fw_net_dmz.network)} ${join(",", sort(google_compute_firewall.fw_net_dmz.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.fw_net_dmz.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.fw_net_dmz.target_tags)}",
      "${google_compute_firewall.fw_net_lan.direction} ${basename(google_compute_firewall.fw_net_lan.network)} ${join(",", sort(google_compute_firewall.fw_net_lan.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.fw_net_lan.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.fw_net_lan.target_tags)}",
      "${google_compute_firewall.fw_ssh.direction} ${basename(google_compute_firewall.fw_ssh.network)} ${join(",", sort(google_compute_firewall.fw_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.fw_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.fw_ssh.target_tags)}",
      ]) == sort([
      "INGRESS outside 10.70.0.0/24 all: isoloom-edge-firewall-t3st1d00-fw",
      "INGRESS dmz 10.70.10.0/24 all: isoloom-edge-firewall-t3st1d00-fw",
      "INGRESS lan 10.70.20.0/24 all: isoloom-edge-firewall-t3st1d00-fw",
      "INGRESS outside 203.0.113.7/32 tcp:22 isoloom-edge-firewall-t3st1d00-fw",
    ])
    error_message = "fw routes: it must admit its three networks and SSH from allowed_cidr, and nothing else"
  }
  assert {
    condition = sort([
      "${google_compute_firewall.web_net_dmz.direction} ${basename(google_compute_firewall.web_net_dmz.network)} ${join(",", sort(google_compute_firewall.web_net_dmz.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_net_dmz.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_net_dmz.target_tags)}",
      "${google_compute_firewall.web_ssh.direction} ${basename(google_compute_firewall.web_ssh.network)} ${join(",", sort(google_compute_firewall.web_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_ssh.target_tags)}",
      "${google_compute_firewall.web_reach_outside.direction} ${basename(google_compute_firewall.web_reach_outside.network)} ${join(",", sort(google_compute_firewall.web_reach_outside.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_reach_outside.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_reach_outside.target_tags)}",
      ]) == sort([
      "INGRESS dmz 10.70.10.0/24 all: isoloom-edge-firewall-t3st1d00-web",
      "INGRESS dmz 203.0.113.7/32 tcp:22 isoloom-edge-firewall-t3st1d00-web",
      "INGRESS dmz 10.70.0.0/24 tcp:80 udp:80 isoloom-edge-firewall-t3st1d00-web",
    ])
    error_message = "web (on dmz) must admit dmz, SSH, and outside on 80 (reach), and nothing else"
  }
  assert {
    condition = sort([
      "${google_compute_firewall.cache_net_lan.direction} ${basename(google_compute_firewall.cache_net_lan.network)} ${join(",", sort(google_compute_firewall.cache_net_lan.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_net_lan.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_net_lan.target_tags)}",
      "${google_compute_firewall.cache_ssh.direction} ${basename(google_compute_firewall.cache_ssh.network)} ${join(",", sort(google_compute_firewall.cache_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_ssh.target_tags)}",
      "${google_compute_firewall.cache_reach_dmz.direction} ${basename(google_compute_firewall.cache_reach_dmz.network)} ${join(",", sort(google_compute_firewall.cache_reach_dmz.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.cache_reach_dmz.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.cache_reach_dmz.target_tags)}",
      ]) == sort([
      "INGRESS lan 10.70.20.0/24 all: isoloom-edge-firewall-t3st1d00-cache",
      "INGRESS lan 203.0.113.7/32 tcp:22 isoloom-edge-firewall-t3st1d00-cache",
      "INGRESS lan 10.70.10.0/24 tcp:6379 udp:6379 isoloom-edge-firewall-t3st1d00-cache",
    ])
    error_message = "cache (on lan) must admit lan, SSH, and dmz on 6379 (reach), and nothing else"
  }
  assert {
    condition = sort([
      "${google_compute_firewall.user_net_outside.direction} ${basename(google_compute_firewall.user_net_outside.network)} ${join(",", sort(google_compute_firewall.user_net_outside.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.user_net_outside.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.user_net_outside.target_tags)}",
      "${google_compute_firewall.user_ssh.direction} ${basename(google_compute_firewall.user_ssh.network)} ${join(",", sort(google_compute_firewall.user_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.user_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.user_ssh.target_tags)}",
      ]) == sort([
      "INGRESS outside 10.70.0.0/24 all: isoloom-edge-firewall-t3st1d00-user",
      "INGRESS outside 203.0.113.7/32 tcp:22 isoloom-edge-firewall-t3st1d00-user",
    ])
    error_message = "user (on outside) must admit outside and SSH from allowed_cidr only"
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
    condition     = keys(output.machines) == ["cache", "fw", "user", "web"] && output.ssh_users == { fw = "admin", web = "admin", cache = "admin", user = "admin" }
    error_message = "`machines` and `ssh_users` must name every machine"
  }
}

# The spec: lan has `internet: false` and `gateway: fw`. During set-up cache reaches the internet
# over its external address; once every machine is ready the runner applies with
# `isoloom_sealed = true` (the manifest's `seal`), and cache then gets two EGRESS firewall rules:
# the lab's own blocks allowed (priority 900), everything else denied (priority 1000). The seal
# is at the network, where root in the guest can't lift it; cache still reaches the other
# networks through fw, and keeps its external address, the way the runner reaches it.
run "a_routed_network_without_internet_is_sealed_at_the_network_after_set_up" {
  command = plan

  variables {
    isoloom_sealed = true
  }

  assert {
    condition     = google_compute_firewall.cache_sealed_lan[0].direction == "EGRESS" && tolist(google_compute_firewall.cache_sealed_lan[0].destination_ranges) == tolist(["0.0.0.0/0"]) && google_compute_firewall.cache_sealed_lan[0].priority == 1000 && [for d in google_compute_firewall.cache_sealed_lan[0].deny : d.protocol] == ["all"]
    error_message = "once sealed, cache (on lan) must deny all egress to the internet"
  }
  assert {
    condition     = toset(google_compute_firewall.cache_lab_lan[0].destination_ranges) == toset(["10.70.0.0/24", "10.70.10.0/24", "10.70.20.0/24"]) && google_compute_firewall.cache_lab_lan[0].priority == 900 && [for a in google_compute_firewall.cache_lab_lan[0].allow : a.protocol] == ["all"]
    error_message = "once sealed, cache may still reach the lab's own networks, a higher-priority allow before the deny"
  }
}
