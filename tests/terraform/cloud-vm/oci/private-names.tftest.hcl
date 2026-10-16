# examples/private-names on Oracle Cloud (cloud-vm): `dns.private` with `corp.internal`, web
# (alias www) and client on one network. The names are a private zone in a private view, which
# the VCN's own resolver attaches. Every run plans against mocked providers; the ids the
# assertions compare are pinned below.

mock_provider "oci" {
  override_during = plan
  source          = "./tests/mocks/oci"
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
  target          = oci_core_vcn.env
  override_during = plan
  values          = { id = "ocid1.vcn.oc1..env" }
}

override_resource {
  target          = oci_dns_view.private
  override_during = plan
  values          = { id = "ocid1.dnsview.oc1..private" }
}

override_resource {
  target          = oci_dns_zone.private
  override_during = plan
  values          = { id = "ocid1.dns-zone.oc1..private" }
}

variables {
  compartment_id       = "ocid1.compartment.oc1..tftest"
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAItftest tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_zone_is_private_in_a_view_the_vcn_s_resolver_attaches" {
  command = plan

  assert {
    condition     = oci_dns_zone.private.name == "corp.internal" && oci_dns_zone.private.scope == "PRIVATE" && oci_dns_zone.private.view_id == "ocid1.dnsview.oc1..private"
    error_message = "the zone must be dns.domain, corp.internal, private, in the environment's private view"
  }
  assert {
    condition     = oci_dns_view.private.scope == "PRIVATE"
    error_message = "the view must be private"
  }
  assert {
    condition     = [for v in oci_dns_resolver.env.attached_views : v.view_id] == ["ocid1.dnsview.oc1..private"] && oci_dns_resolver.env.scope == "PRIVATE"
    error_message = "the resolver must attach the private view, or the zone answers nobody"
  }
  # The resolver's id comes from the VCN's resolver association, a data source Terraform reads
  # only once the VCN exists: the plan can say which VCN it asks about, not the id it gets.
  assert {
    condition     = data.oci_core_vcn_dns_resolver_association.env.vcn_id == "ocid1.vcn.oc1..env"
    error_message = "the resolver must be the environment's VCN's"
  }
}

run "every_machine_and_alias_has_its_address_in_the_zone" {
  command = plan

  assert {
    condition = sort(flatten([
      [for i in oci_dns_rrset.web_corp_internal.items : "${oci_dns_rrset.web_corp_internal.zone_name_or_id} ${i.domain} ${i.rtype} ${i.rdata}"],
      [for i in oci_dns_rrset.www_corp_internal.items : "${oci_dns_rrset.www_corp_internal.zone_name_or_id} ${i.domain} ${i.rtype} ${i.rdata}"],
      [for i in oci_dns_rrset.client_corp_internal.items : "${oci_dns_rrset.client_corp_internal.zone_name_or_id} ${i.domain} ${i.rtype} ${i.rdata}"],
      ])) == sort([
      "ocid1.dns-zone.oc1..private web.corp.internal A 10.84.0.10",
      "ocid1.dns-zone.oc1..private www.corp.internal A 10.84.0.10",
      "ocid1.dns-zone.oc1..private client.corp.internal A 10.84.0.20",
    ])
    error_message = "web, its alias www and client must each have an A record at their address in the private zone"
  }
}

run "the_network_admits_itself_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in oci_core_security_list.office.ingress_security_rules : join(" ", concat([r.protocol, r.source], [for o in r.tcp_options : "${o.min}-${o.max}"], [for o in r.udp_options : "${o.min}-${o.max}"], [for o in r.icmp_options : "type ${o.type} code ${o.code}"]))]) == sort([
      "all 10.84.0.0/24",
      "6 203.0.113.7/32 22-22",
      "1 0.0.0.0/0 type 3 code 4",
    ])
    error_message = "office must admit office, SSH from allowed_cidr and ICMP's fragmentation-needed only: web publishes nothing"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file"
  }
}
