# examples/private-names on Google Cloud (cloud-vm): `dns.private` with `corp.internal`, web
# (alias www) and client on one network. The names are a Cloud DNS private zone visible to the
# network's VPC. The environment uses an existing project (`project`). Every run plans against
# mocked providers; the ids the assertions compare are pinned below, in Google's forms.

mock_provider "google" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = google_compute_network.office
  override_during = plan
  values = {
    id        = "projects/tftest/global/networks/office"
    self_link = "https://www.googleapis.com/compute/v1/projects/tftest/global/networks/office"
  }
}

override_resource {
  target          = google_compute_subnetwork.office
  override_during = plan
  values          = { id = "projects/tftest/regions/europe-west9/subnetworks/office" }
}

variables {
  project              = "tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
}

run "the_zone_is_private_to_the_network_s_vpc" {
  command = plan

  assert {
    condition     = google_dns_managed_zone.private.dns_name == "corp.internal." && google_dns_managed_zone.private.visibility == "private"
    error_message = "the zone must be dns.domain, corp.internal, private"
  }
  assert {
    condition     = flatten([for c in google_dns_managed_zone.private.private_visibility_config : [for n in c.networks : n.network_url]]) == ["projects/tftest/global/networks/office"]
    error_message = "the zone must be visible to office's VPC network, or it answers nobody"
  }
}

run "every_machine_and_alias_has_its_address_in_the_zone" {
  command = plan

  assert {
    condition = sort([
      "${google_dns_record_set.web_corp_internal.managed_zone} ${google_dns_record_set.web_corp_internal.name} ${google_dns_record_set.web_corp_internal.type} ${join(",", google_dns_record_set.web_corp_internal.rrdatas)}",
      "${google_dns_record_set.www_corp_internal.managed_zone} ${google_dns_record_set.www_corp_internal.name} ${google_dns_record_set.www_corp_internal.type} ${join(",", google_dns_record_set.www_corp_internal.rrdatas)}",
      "${google_dns_record_set.client_corp_internal.managed_zone} ${google_dns_record_set.client_corp_internal.name} ${google_dns_record_set.client_corp_internal.type} ${join(",", google_dns_record_set.client_corp_internal.rrdatas)}",
      ]) == sort([
      "isoloom-private-names-t3st1d00-private web.corp.internal. A 10.84.0.10",
      "isoloom-private-names-t3st1d00-private www.corp.internal. A 10.84.0.10",
      "isoloom-private-names-t3st1d00-private client.corp.internal. A 10.84.0.20",
    ])
    error_message = "web, its alias www and client must each have an A record at their address in the private zone"
  }
}

run "the_machines_admit_their_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([
      "${google_compute_firewall.web_net_office.direction} ${basename(google_compute_firewall.web_net_office.network)} ${join(",", sort(google_compute_firewall.web_net_office.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_net_office.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_net_office.target_tags)}",
      "${google_compute_firewall.web_ssh.direction} ${basename(google_compute_firewall.web_ssh.network)} ${join(",", sort(google_compute_firewall.web_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.web_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.web_ssh.target_tags)}",
      ]) == sort([
      "INGRESS office 10.84.0.0/24 all: isoloom-private-names-t3st1d00-web",
      "INGRESS office 203.0.113.7/32 tcp:22 isoloom-private-names-t3st1d00-web",
    ])
    error_message = "web publishes nothing: it must admit office and SSH from allowed_cidr only"
  }
  assert {
    condition = sort([
      "${google_compute_firewall.client_net_office.direction} ${basename(google_compute_firewall.client_net_office.network)} ${join(",", sort(google_compute_firewall.client_net_office.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.client_net_office.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.client_net_office.target_tags)}",
      "${google_compute_firewall.client_ssh.direction} ${basename(google_compute_firewall.client_ssh.network)} ${join(",", sort(google_compute_firewall.client_ssh.source_ranges))} ${join(" ", sort([for a in google_compute_firewall.client_ssh.allow : "${a.protocol}:${join(",", a.ports == null ? [] : a.ports)}"]))} ${join(",", google_compute_firewall.client_ssh.target_tags)}",
      ]) == sort([
      "INGRESS office 10.84.0.0/24 all: isoloom-private-names-t3st1d00-client",
      "INGRESS office 203.0.113.7/32 tcp:22 isoloom-private-names-t3st1d00-client",
    ])
    error_message = "client must admit office and SSH from allowed_cidr only"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file"
  }
}
