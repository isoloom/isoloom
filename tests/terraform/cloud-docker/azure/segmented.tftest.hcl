# examples/segmented on Azure (cloud-docker): one host VM in a virtual network of its own runs the
# docker target's Compose file, so the lab's three networks, its `reach` rules and `back` without
# internet live inside Compose; the cloud's part is
# the host's subnet, its public address (its way out and in), and the security group on its
# interface: SSH from allowed_cidr, nothing else, since no port is published. The host is alone in
# its virtual network, so Azure's own default inbound rules (the virtual network, the load
# balancer's probes) open it to nothing else. Every run plans against mocked providers; the ids
# the assertions compare are pinned below.

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
  target          = azurerm_subnet.env
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/env" }
}

override_resource {
  target          = azurerm_public_ip.env
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/env", ip_address = "198.51.100.10" }
}

override_resource {
  target          = azurerm_network_security_group.env
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/env" }
}

override_resource {
  target          = azurerm_network_interface.env
  override_during = plan
  values          = { id = "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/env" }
}

variables {
  allowed_cidr         = "203.0.113.7/32"
  ssh_public_key       = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDwvyPBIAHawI6izLXPHSZzyxevpbnDLLk4uonfaF96l tf-test"
  ssh_private_key_file = "/dev/null"
}

run "the_host_sits_on_the_one_subnet_of_its_network_with_a_public_address" {
  command = plan

  assert {
    condition     = tolist(azurerm_virtual_network.env.address_space) == tolist(["10.42.0.0/16"]) && tolist(azurerm_subnet.env.address_prefixes) == tolist(["10.42.1.0/24"])
    error_message = "the host's subnet must be 10.42.1.0/24, in the environment's virtual network (10.42.0.0/16)"
  }
  assert {
    condition     = azurerm_subnet.env.virtual_network_name == azurerm_virtual_network.env.name
    error_message = "the host's subnet must be in the environment's virtual network"
  }
  assert {
    condition     = [for c in azurerm_network_interface.env.ip_configuration : "${c.subnet_id} ${c.public_ip_address_id}"] == ["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/env/subnets/env /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/env"]
    error_message = "the host's one interface must be on the environment's subnet, with the public address (its way out: Docker's install, and Terraform's SSH)"
  }
  assert {
    condition     = tolist(azurerm_linux_virtual_machine.env.network_interface_ids) == tolist(["/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/env"])
    error_message = "the host must have that interface only"
  }
}

run "the_host_admits_ssh_from_allowed_cidr_only" {
  command = plan

  assert {
    condition     = azurerm_network_interface_security_group_association.env.network_interface_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkInterfaces/env" && azurerm_network_interface_security_group_association.env.network_security_group_id == "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg/providers/Microsoft.Network/networkSecurityGroups/env"
    error_message = "the environment's security group must be on the host's interface"
  }
  assert {
    condition = sort([for r in azurerm_network_security_group.env.security_rule : "${r.direction} ${r.access} ${r.protocol} ${join(",", compact(concat([r.destination_port_range != null ? r.destination_port_range : ""], r.destination_port_ranges != null ? tolist(r.destination_port_ranges) : [])))} ${r.source_address_prefix}" if r.direction == "Inbound"]) == sort([
      "Inbound Allow Tcp 22 203.0.113.7/32",
    ])
    error_message = "the host must admit SSH from allowed_cidr, and nothing else: segmented publishes no port"
  }
}

run "the_outputs_name_the_host_and_the_ready_file" {
  command = plan

  assert {
    condition     = output.ip == "198.51.100.10"
    error_message = "`ip` must be the host's public address"
  }
  assert {
    condition     = output.ssh_user == "isoloom"
    error_message = "`ssh_user` must be the admin user the module creates"
  }
  assert {
    condition     = output.ready_file == "/var/lib/isoloom/ready"
    error_message = "`ready_file` must be the isoloom-ready/1 file the set-up writes"
  }
}
