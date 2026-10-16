# examples/solo-web on Google Cloud (cloud-vm): one machine on one network, its web port (80,
# http) published as 8080. A machine's firewall rules target its tag: the suite names every rule
# the module writes for it. The environment uses an existing project (`project`), so no project
# is created. Every run plans against mocked providers; the ids and the address the assertions
# compare are pinned below, in Google's forms (the provider checks some while planning).

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
  target          = google_compute_network.app
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/app"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/app"
  }
}

override_resource {
  target          = google_compute_subnetwork.app
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/app" }
}

override_resource {
  target          = google_compute_instance.web
  override_during = plan
  values = {
    # A list of blocks takes one object, which every element gets: web has one interface.
    network_interface = {
      access_config = { nat_ip = "198.51.100.10" }
    }
  }
}

variables {
  project              = "tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_network_is_a_vpc_network_of_its_own_that_reaches_the_internet" {
  command = plan

  assert {
    condition     = google_compute_subnetwork.app.ip_cidr_range == "10.80.0.0/24" && google_compute_subnetwork.app.network == "projects/tftest/global/networks/app"
    error_message = "app must be a subnetwork at the network's exact range, in app's own VPC network"
  }
  assert {
    condition     = google_compute_network.app.auto_create_subnetworks == false
    error_message = "app's VPC network must hold only the environment's subnetwork"
  }
  assert {
    condition     = coalesce(google_compute_network.app.delete_default_routes_on_create, false) == false
    error_message = "app (internet: true) must keep its VPC network's default route to the internet"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_only" {
  command = plan

  assert {
    condition     = google_compute_instance.web.network_interface[0].subnetwork == "projects/tftest/regions/europe-west9/subnetworks/app" && google_compute_instance.web.network_interface[0].network_ip == "10.80.0.10" && tolist(google_compute_instance.web.tags) == tolist(["isoloom-solo-web-t3st1d00-web"])
    error_message = "web must sit on app at 10.80.0.10, tagged as itself only"
  }
  assert {
    condition = sort([
      "${google_compute_firewall.web_net_app.direction} ${basename(google_compute_firewall.web_net_app.network)} ${join(",", sort(google_compute_firewall.web_net_app.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_net_app.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_net_app.target_tags)}",
      "${google_compute_firewall.web_ssh.direction} ${basename(google_compute_firewall.web_ssh.network)} ${join(",", sort(google_compute_firewall.web_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_ssh.target_tags)}",
      "${google_compute_firewall.web_published.direction} ${basename(google_compute_firewall.web_published.network)} ${join(",", sort(google_compute_firewall.web_published.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_published.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_published.target_tags)}",
      ]) == sort([
      "INGRESS app 10.80.0.0/24 all: isoloom-solo-web-t3st1d00-web",
      "INGRESS app 203.0.113.7/32 tcp:22 isoloom-solo-web-t3st1d00-web",
      "INGRESS app 203.0.113.7/32 tcp:8080 isoloom-solo-web-t3st1d00-web",
    ])
    error_message = "web must admit its network, SSH and its published port (8080/tcp) from allowed_cidr, and nothing else"
  }
}

run "the_published_port_is_an_output_keyed_by_machine_and_port_name" {
  command = plan

  assert {
    condition     = output.published == { "web/web" = "198.51.100.10:8080" }
    error_message = "`published` must hold web's port `web` at its external address and published port"
  }
  assert {
    condition     = output.ip == "198.51.100.10" && output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ip` must be web's external address and `ready_file` the isoloom-ready/1 file"
  }
}
