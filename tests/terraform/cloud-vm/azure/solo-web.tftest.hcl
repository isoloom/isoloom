# examples/solo-web on Azure (cloud-vm): one machine on one network, its web port (80, http)
# published as 8080. Every run plans against mocked providers; the ids the assertions compare are
# pinned below, as Azure resource ids (the provider checks their form while planning).

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
  target          = azurerm_subnet.app
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/app" }
}

override_resource {
  target          = azurerm_network_security_group.web
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/web" }
}

override_resource {
  target          = azurerm_network_interface.web_app
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/web-app" }
}

override_resource {
  target          = azurerm_public_ip.web
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/web", ip_address = "198.51.100.10" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_network_is_a_subnet_with_its_range_and_internet" {
  command = plan

  assert {
    condition     = tolist(azurerm_virtual_network.env.address_space) == tolist(["10.80.0.0/24"]) && tolist(azurerm_subnet.app.address_prefixes) == tolist(["10.80.0.0/24"])
    error_message = "app's subnet must hold the network's exact range, the virtual network's only block"
  }
  assert {
    condition     = azurerm_subnet.app.default_outbound_access_enabled == true
    error_message = "app (internet: true) must have Azure's default outbound access"
  }
}

run "the_web_server_publishes_8080_to_allowed_cidr_only" {
  command = plan

  assert {
    condition     = azurerm_network_interface.web_app.ip_configuration[0].subnet_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/app" && azurerm_network_interface.web_app.ip_configuration[0].private_ip_address == "10.80.0.10"
    error_message = "web's interface must sit on app at 10.80.0.10"
  }
  assert {
    condition     = azurerm_network_interface_security_group_association.web_app.network_interface_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/web-app" && azurerm_network_interface_security_group_association.web_app.network_security_group_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/web"
    error_message = "web's interface must be behind web's own network security group"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.web.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.80.0.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Allow Tcp 8080 203.0.113.7/32",
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
