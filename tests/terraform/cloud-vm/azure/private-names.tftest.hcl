# examples/private-names on Azure (cloud-vm): `dns.private` with `corp.internal`, web (alias www)
# and client on one network. The names are an Azure private DNS zone linked to the virtual
# network, with a reverse zone for the network's block.

mock_provider "azurerm" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = azurerm_virtual_network.env
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env" }
}

override_resource {
  target          = azurerm_private_dns_zone.private
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/corp.internal" }
}

override_resource {
  target          = azurerm_private_dns_zone.reverse_0_84_10
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/0.84.10.in-addr.arpa" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
}

run "the_zone_is_linked_to_the_virtual_network" {
  command = plan

  assert {
    condition     = azurerm_private_dns_zone.private.name == "corp.internal"
    error_message = "the zone must be dns.domain, corp.internal"
  }
  assert {
    condition     = azurerm_private_dns_zone_virtual_network_link.private.private_dns_zone_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/corp.internal" && azurerm_private_dns_zone_virtual_network_link.private.virtual_network_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env"
    error_message = "the zone must be linked to the environment's virtual network, which then resolves through it"
  }
}

run "every_machine_and_alias_has_its_address_in_the_zone" {
  command = plan

  assert {
    condition = sort([
      "${azurerm_private_dns_a_record.web_corp_internal.name} ${join(",", azurerm_private_dns_a_record.web_corp_internal.records)} ${azurerm_private_dns_a_record.web_corp_internal.private_dns_zone_id}",
      "${azurerm_private_dns_a_record.www_corp_internal.name} ${join(",", azurerm_private_dns_a_record.www_corp_internal.records)} ${azurerm_private_dns_a_record.www_corp_internal.private_dns_zone_id}",
      "${azurerm_private_dns_a_record.client_corp_internal.name} ${join(",", azurerm_private_dns_a_record.client_corp_internal.records)} ${azurerm_private_dns_a_record.client_corp_internal.private_dns_zone_id}",
      ]) == sort([
      "web 10.84.0.10 /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/corp.internal",
      "www 10.84.0.10 /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/corp.internal",
      "client 10.84.0.20 /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/privateDnsZones/corp.internal",
    ])
    error_message = "web, its alias www and client must each have an A record at their address in the zone"
  }
}

run "each_address_resolves_back_to_its_machine" {
  command = plan

  assert {
    condition     = azurerm_private_dns_zone.reverse_0_84_10.name == "0.84.10.in-addr.arpa" && azurerm_private_dns_zone_virtual_network_link.reverse_0_84_10.virtual_network_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env"
    error_message = "office's reverse zone must be linked to the virtual network"
  }
  assert {
    condition = sort([
      "${azurerm_private_dns_ptr_record.web_corp_internal.name} ${join(",", azurerm_private_dns_ptr_record.web_corp_internal.records)}",
      "${azurerm_private_dns_ptr_record.client_corp_internal.name} ${join(",", azurerm_private_dns_ptr_record.client_corp_internal.records)}",
    ]) == sort(["10 web.corp.internal", "20 client.corp.internal"])
    error_message = "10.84.0.10 must resolve back to web.corp.internal and .20 to client.corp.internal"
  }
}

run "the_machines_admit_their_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in azurerm_network_security_group.web.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.84.0.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Deny * * VirtualNetwork",
    ])
    error_message = "web publishes nothing: it must admit office and SSH from allowed_cidr, then deny the rest of the virtual network"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.client.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.84.0.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Deny * * VirtualNetwork",
    ])
    error_message = "client must admit office and SSH from allowed_cidr, then deny the rest of the virtual network"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file"
  }
}
