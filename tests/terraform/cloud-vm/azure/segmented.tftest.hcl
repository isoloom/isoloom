# examples/segmented on Azure (cloud-vm): three networks as subnets of one virtual network,
# `reach` from access to front (every port) and from front to back (6379 only), `back` without
# internet. Every run plans against mocked providers; the ids the assertions compare are pinned
# below, as Azure resource ids (the provider checks their form while planning).

mock_provider "azurerm" {
  override_during = plan
}

override_resource {
  target          = terraform_data.id
  override_during = plan
  values          = { output = "t3st1d00" }
}

override_resource {
  target          = azurerm_subnet.front
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/front" }
}

override_resource {
  target          = azurerm_subnet.back
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/back" }
}

override_resource {
  target          = azurerm_subnet.access
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/access" }
}

override_resource {
  target          = azurerm_network_security_group.cache
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/cache" }
}

override_resource {
  target          = azurerm_network_security_group.web
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/web" }
}

override_resource {
  target          = azurerm_network_security_group.user
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/user" }
}

override_resource {
  target          = azurerm_network_interface.cache_back
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/cache-back" }
}

override_resource {
  target          = azurerm_network_interface.web_front
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/web-front" }
}

override_resource {
  target          = azurerm_network_interface.user_access
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/user-access" }
}

override_resource {
  target          = azurerm_public_ip.cache
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/cache", ip_address = "198.51.100.20" }
}

override_resource {
  target          = azurerm_public_ip.web
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/web", ip_address = "198.51.100.10" }
}

override_resource {
  target          = azurerm_public_ip.user
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/user", ip_address = "198.51.100.30" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
}

run "each_network_is_a_subnet_of_the_one_vnet_at_its_exact_range" {
  command = plan

  assert {
    condition     = sort(azurerm_virtual_network.env.address_space) == sort(["10.61.10.0/24", "10.61.20.0/24", "10.61.99.0/24"])
    error_message = "the virtual network's address space must be exactly the three networks' blocks"
  }
  assert {
    condition     = tolist(azurerm_subnet.front.address_prefixes) == tolist(["10.61.10.0/24"]) && tolist(azurerm_subnet.back.address_prefixes) == tolist(["10.61.20.0/24"]) && tolist(azurerm_subnet.access.address_prefixes) == tolist(["10.61.99.0/24"])
    error_message = "each network's subnet must have the network's exact range"
  }
  assert {
    condition     = toset([azurerm_subnet.front.virtual_network_name, azurerm_subnet.back.virtual_network_name, azurerm_subnet.access.virtual_network_name]) == toset([azurerm_virtual_network.env.name])
    error_message = "every network's subnet must be in the environment's one virtual network"
  }
}

run "only_the_networks_with_internet_get_default_outbound_access" {
  command = plan

  assert {
    condition     = [azurerm_subnet.front.default_outbound_access_enabled, azurerm_subnet.back.default_outbound_access_enabled, azurerm_subnet.access.default_outbound_access_enabled] == [true, false, true]
    error_message = "front and access (internet: true) must have Azure's default outbound access, back (internet: false) must not"
  }
}

run "each_machine_is_on_its_network_at_its_address_behind_its_own_nsg" {
  command = plan

  assert {
    condition     = azurerm_network_interface.cache_back.ip_configuration[0].subnet_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/back" && azurerm_network_interface.cache_back.ip_configuration[0].private_ip_address == "10.61.20.20" && azurerm_network_interface.cache_back.ip_configuration[0].private_ip_address_allocation == "Static"
    error_message = "cache's interface must sit on back at the static address 10.61.20.20"
  }
  assert {
    condition     = azurerm_network_interface.web_front.ip_configuration[0].subnet_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/front" && azurerm_network_interface.web_front.ip_configuration[0].private_ip_address == "10.61.10.10" && azurerm_network_interface.web_front.ip_configuration[0].private_ip_address_allocation == "Static"
    error_message = "web's interface must sit on front at the static address 10.61.10.10"
  }
  assert {
    condition     = azurerm_network_interface.user_access.ip_configuration[0].subnet_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/access" && azurerm_network_interface.user_access.ip_configuration[0].private_ip_address == "10.61.99.10" && azurerm_network_interface.user_access.ip_configuration[0].private_ip_address_allocation == "Static"
    error_message = "user's interface must sit on access at the static address 10.61.99.10"
  }
  assert {
    condition     = tolist(azurerm_linux_virtual_machine.cache.network_interface_ids) == tolist(["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/cache-back"]) && tolist(azurerm_linux_virtual_machine.web.network_interface_ids) == tolist(["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/web-front"]) && tolist(azurerm_linux_virtual_machine.user.network_interface_ids) == tolist(["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/user-access"])
    error_message = "each machine must have its one interface on its network"
  }
  assert {
    condition     = [azurerm_network_interface_security_group_association.cache_back.network_interface_id, azurerm_network_interface_security_group_association.cache_back.network_security_group_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/cache-back", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/cache"]
    error_message = "cache's interface must be behind cache's own network security group"
  }
  assert {
    condition     = [azurerm_network_interface_security_group_association.web_front.network_interface_id, azurerm_network_interface_security_group_association.web_front.network_security_group_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/web-front", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/web"]
    error_message = "web's interface must be behind web's own network security group"
  }
  assert {
    condition     = [azurerm_network_interface_security_group_association.user_access.network_interface_id, azurerm_network_interface_security_group_association.user_access.network_security_group_id] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/user-access", "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/user"]
    error_message = "user's interface must be behind user's own network security group"
  }
}

run "the_cache_admits_its_network_ssh_and_front_on_6379_only" {
  command = plan

  assert {
    condition = sort([for r in azurerm_network_security_group.cache.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.61.20.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Allow Tcp 6379 10.61.10.0/24",
      "Allow Udp 6379 10.61.10.0/24",
      "Deny * * VirtualNetwork",
    ])
    error_message = "cache (on back) must admit back, SSH from allowed_cidr and front on 6379 (reach), then deny the rest of the virtual network, and nothing else"
  }
}

run "the_web_server_admits_its_network_ssh_and_access_on_every_port" {
  command = plan

  assert {
    condition = sort([for r in azurerm_network_security_group.web.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.61.10.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Allow * * 10.61.99.0/24",
      "Deny * * VirtualNetwork",
    ])
    error_message = "web (on front) must admit front, SSH from allowed_cidr and access on every port (reach), then deny the rest of the virtual network, and nothing else: no rule from back"
  }
}

run "the_user_s_machine_admits_its_network_and_ssh_only" {
  command = plan

  assert {
    condition = sort([for r in azurerm_network_security_group.user.security_rule : "${r.access} ${r.protocol} ${r.destination_port_range} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Allow * * 10.61.99.0/24",
      "Allow Tcp 22 203.0.113.7/32",
      "Deny * * VirtualNetwork",
    ])
    error_message = "user (on access) must admit access and SSH from allowed_cidr, then deny the rest of the virtual network: no reach rule leads into access"
  }
}

run "the_outputs_name_the_way_in_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.30"
    error_message = "`ip` must be the access machine's (user's) public address"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
  assert {
    condition     = output.machines == { cache = "198.51.100.20", web = "198.51.100.10", user = "198.51.100.30" }
    error_message = "`machines` must give every machine's public address"
  }
  assert {
    condition     = output.ssh_users == { cache = "isoloom", web = "isoloom", user = "isoloom" }
    error_message = "`ssh_users` must name every machine's SSH user"
  }
}

# The spec: everything between networks is blocked but `reach`. Azure gives every network
# security group the default rule AllowVnetInBound (priority 65000): any address of the virtual
# network may reach the machine on every port. The module closes this by adding, after each
# machine's Allow rules, a Deny rule for the rest of the virtual network inbound, so segmentation
# and `reach` hold against Azure's own default.
run "every_group_denies_the_rest_of_the_vnet_inbound_after_its_allow_rules" {
  command = plan

  assert {
    condition     = length([for r in azurerm_network_security_group.cache.security_rule : r if r.direction == "Inbound" && r.access == "Deny" && contains(["*", "VirtualNetwork"], r.source_address_prefix) && r.priority > max([for a in azurerm_network_security_group.cache.security_rule : a.priority if a.direction == "Inbound" && a.access == "Allow"]...)]) > 0
    error_message = "cache's group must deny the rest of the virtual network inbound, after its Allow rules (Azure's AllowVnetInBound admits it otherwise)"
  }
  assert {
    condition     = length([for r in azurerm_network_security_group.web.security_rule : r if r.direction == "Inbound" && r.access == "Deny" && contains(["*", "VirtualNetwork"], r.source_address_prefix) && r.priority > max([for a in azurerm_network_security_group.web.security_rule : a.priority if a.direction == "Inbound" && a.access == "Allow"]...)]) > 0
    error_message = "web's group must deny the rest of the virtual network inbound, after its Allow rules (Azure's AllowVnetInBound admits it otherwise)"
  }
  assert {
    condition     = length([for r in azurerm_network_security_group.user.security_rule : r if r.direction == "Inbound" && r.access == "Deny" && contains(["*", "VirtualNetwork"], r.source_address_prefix) && r.priority > max([for a in azurerm_network_security_group.user.security_rule : a.priority if a.direction == "Inbound" && a.access == "Allow"]...)]) > 0
    error_message = "user's group must deny the rest of the virtual network inbound, after its Allow rules (Azure's AllowVnetInBound admits it otherwise)"
  }
}

# The spec: `back` has `internet: false`, so once its machines are set up they may not reach the
# internet. During set-up cache reaches the internet over its Standard public address; once every
# machine is ready the runner applies with `isoloom_sealed = true` (the manifest's `seal`), and
# cache's group then denies the internet outbound (the virtual network stays reachable, Azure's
# AllowVnetOutBound). The seal is egress-only at the network, where root in the guest can't lift
# it; cache keeps its public address, the way the runner reaches it.
run "a_network_without_internet_is_sealed_at_the_network_after_set_up" {
  command = plan

  variables {
    isoloom_sealed = true
  }

  assert {
    condition     = [for r in azurerm_network_security_group.cache.security_rule : "${r.access} ${r.destination_address_prefix}" if r.direction == "Outbound"] == ["Deny Internet"]
    error_message = "once sealed, cache (on back, internet: false) must deny the internet outbound, at the network"
  }
  assert {
    condition     = [for r in azurerm_network_security_group.web.security_rule : "${r.access} ${r.destination_address_prefix}" if r.direction == "Outbound"] == ["Allow *"]
    error_message = "web (on front, internet: true) keeps its way out: the seal only closes the machines of an internet: false network"
  }
}
