# examples/edge-firewall on Azure (cloud-vm): `fw` routes dmz and lan (`gateway: fw`), so each
# network's route table sends the others to fw's address there (a virtual appliance); `reach`
# opens outside to dmz on 80 and dmz to lan on 6379; lan has no internet. Azure keeps a subnet's
# first addresses, so fw takes .254 on the networks it routes. Every run plans against mocked
# providers; the ids the assertions compare are pinned below, as Azure resource ids.

mock_provider "azurerm" {
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
  target          = azurerm_subnet.outside
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/outside" }
}

override_resource {
  target          = azurerm_route_table.outside
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/outside" }
}

override_resource {
  target          = azurerm_network_interface.fw_outside
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-outside" }
}

override_resource {
  target          = azurerm_subnet.dmz
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/dmz" }
}

override_resource {
  target          = azurerm_route_table.dmz
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/dmz" }
}

override_resource {
  target          = azurerm_network_interface.fw_dmz
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-dmz" }
}

override_resource {
  target          = azurerm_subnet.lan
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/lan" }
}

override_resource {
  target          = azurerm_route_table.lan
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/lan" }
}

override_resource {
  target          = azurerm_network_interface.fw_lan
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-lan" }
}

override_resource {
  target          = azurerm_public_ip.cache
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/cache" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
}

run "each_network_is_a_subnet_at_its_exact_range" {
  command = plan

  assert {
    condition     = [tolist(azurerm_subnet.outside.address_prefixes), tolist(azurerm_subnet.dmz.address_prefixes), tolist(azurerm_subnet.lan.address_prefixes)] == [tolist(["10.70.0.0/24"]), tolist(["10.70.10.0/24"]), tolist(["10.70.20.0/24"])]
    error_message = "each network's subnet must hold the network's exact range"
  }
  assert {
    condition     = [azurerm_subnet.outside.default_outbound_access_enabled, azurerm_subnet.dmz.default_outbound_access_enabled, azurerm_subnet.lan.default_outbound_access_enabled] == [true, true, false]
    error_message = "outside and dmz (internet: true) must have Azure's default outbound access, lan (internet: false) must not"
  }
}

run "each_network_has_its_table_sending_the_others_through_fw" {
  command = plan

  assert {
    condition     = [azurerm_subnet_route_table_association.outside.route_table_id, azurerm_subnet_route_table_association.dmz.route_table_id, azurerm_subnet_route_table_association.lan.route_table_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/outside", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/dmz", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/routeTables/lan"]
    error_message = "each network's subnet must use its own route table"
  }
  assert {
    condition     = [azurerm_subnet_route_table_association.outside.subnet_id, azurerm_subnet_route_table_association.dmz.subnet_id, azurerm_subnet_route_table_association.lan.subnet_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/outside", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/dmz", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/lan"]
    error_message = "each route table must be on its network's subnet"
  }
  assert {
    condition = sort([
      "${azurerm_route.outside_dmz.route_table_name} ${azurerm_route.outside_dmz.address_prefix} ${azurerm_route.outside_dmz.next_hop_type} ${azurerm_route.outside_dmz.next_hop_in_ip_address}",
      "${azurerm_route.outside_lan.route_table_name} ${azurerm_route.outside_lan.address_prefix} ${azurerm_route.outside_lan.next_hop_type} ${azurerm_route.outside_lan.next_hop_in_ip_address}",
      "${azurerm_route.dmz_outside.route_table_name} ${azurerm_route.dmz_outside.address_prefix} ${azurerm_route.dmz_outside.next_hop_type} ${azurerm_route.dmz_outside.next_hop_in_ip_address}",
      "${azurerm_route.dmz_lan.route_table_name} ${azurerm_route.dmz_lan.address_prefix} ${azurerm_route.dmz_lan.next_hop_type} ${azurerm_route.dmz_lan.next_hop_in_ip_address}",
      "${azurerm_route.lan_outside.route_table_name} ${azurerm_route.lan_outside.address_prefix} ${azurerm_route.lan_outside.next_hop_type} ${azurerm_route.lan_outside.next_hop_in_ip_address}",
      "${azurerm_route.lan_dmz.route_table_name} ${azurerm_route.lan_dmz.address_prefix} ${azurerm_route.lan_dmz.next_hop_type} ${azurerm_route.lan_dmz.next_hop_in_ip_address}",
      ]) == sort([
      "isoloom-edge-firewall-t3st1d00-outside 10.70.10.0/24 VirtualAppliance 10.70.0.5",
      "isoloom-edge-firewall-t3st1d00-outside 10.70.20.0/24 VirtualAppliance 10.70.0.5",
      "isoloom-edge-firewall-t3st1d00-dmz 10.70.0.0/24 VirtualAppliance 10.70.10.254",
      "isoloom-edge-firewall-t3st1d00-dmz 10.70.20.0/24 VirtualAppliance 10.70.10.254",
      "isoloom-edge-firewall-t3st1d00-lan 10.70.0.0/24 VirtualAppliance 10.70.20.254",
      "isoloom-edge-firewall-t3st1d00-lan 10.70.10.0/24 VirtualAppliance 10.70.20.254",
    ])
    error_message = "each network must reach the other two through fw's address on it"
  }
}

run "fw_forwards_on_an_interface_per_network" {
  command = plan

  assert {
    condition     = [azurerm_network_interface.fw_outside.ip_configuration[0].subnet_id, azurerm_network_interface.fw_dmz.ip_configuration[0].subnet_id, azurerm_network_interface.fw_lan.ip_configuration[0].subnet_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/outside", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/dmz", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/lan"]
    error_message = "fw must have one interface on each of its networks"
  }
  assert {
    condition     = [azurerm_network_interface.fw_outside.ip_configuration[0].private_ip_address, azurerm_network_interface.fw_dmz.ip_configuration[0].private_ip_address, azurerm_network_interface.fw_lan.ip_configuration[0].private_ip_address] == ["10.70.0.5", "10.70.10.254", "10.70.20.254"]
    error_message = "fw must be at 10.70.0.5 on outside, and at .254 on dmz and lan (Azure keeps the gateway address .1)"
  }
  assert {
    condition     = azurerm_network_interface.fw_outside.ip_forwarding_enabled && azurerm_network_interface.fw_dmz.ip_forwarding_enabled && azurerm_network_interface.fw_lan.ip_forwarding_enabled
    error_message = "fw's interfaces must forward, or Azure drops what fw routes"
  }
  assert {
    condition     = tolist(azurerm_linux_virtual_machine.fw.network_interface_ids) == tolist(["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-outside", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-dmz", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/fw-lan"])
    error_message = "fw's first interface must be on outside (its public address), then dmz and lan"
  }
}

run "each_machine_admits_its_networks_ssh_and_its_reach_rules_only" {
  command = plan

  assert {
    condition = sort([for r in azurerm_network_security_group.fw.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.70.0.0/24",
      "Allow * * 10.70.10.0/24",
      "Allow * * 10.70.20.0/24",
      "Allow Tcp 22 203.0.113.7/32",
    ])
    error_message = "fw routes: it must admit its three networks and SSH from allowed_cidr, and nothing else"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.web.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.70.10.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Allow Tcp 80 10.70.0.0/24",
      "Allow Udp 80 10.70.0.0/24",
    ])
    error_message = "web (on dmz) must admit dmz, SSH, and outside on 80 (reach), and nothing else"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.cache.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.70.20.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Allow Tcp 6379 10.70.10.0/24",
      "Allow Udp 6379 10.70.10.0/24",
    ])
    error_message = "cache (on lan) must admit lan, SSH, and dmz on 6379 (reach), and nothing else"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.user.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.70.0.0/24",
      "Allow Tcp 22 203.0.113.7/32",
    ])
    error_message = "user (on outside) must admit outside and SSH from allowed_cidr only"
  }
}

# The spec: lan has `internet: false` and `gateway: fw`. Its subnet has no default outbound
# access, but cache's interface carries a Standard public address (the machines are set up over
# SSH there), which gives it a way to the internet that never crosses fw.
run "known_bug_a_routed_network_without_internet_goes_out_around_its_gateway" {
  command = plan

  assert {
    condition     = azurerm_network_interface.cache_lan.ip_configuration[0].public_ip_address_id == null
    error_message = "cache, on lan (internet: false), must have no public address"
  }
}
