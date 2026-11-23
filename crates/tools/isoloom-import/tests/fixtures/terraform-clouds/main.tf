# A VM of each other cloud the import reads: Azure (through its NIC), Google, Proxmox.
resource "azurerm_network_interface" "dc" {
  ip_configuration {
    private_ip_address = "10.50.1.10"
  }
}
resource "azurerm_windows_virtual_machine" "dc" {
  size                  = "Standard_B9"
  network_interface_ids = [azurerm_network_interface.dc.id]
}
resource "google_compute_instance" "web" {
  machine_type = "e2-standard-4"
  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-11"
    }
  }
  network_interface {
    network_ip = "10.50.1.20"
  }
}
resource "proxmox_virtual_environment_vm" "db" {
  name = "db"
  operating_system {
    type = "l26"
  }
  cpu {
    cores = 2
  }
  memory {
    dedicated = 2048
  }
  initialization {
    ip_config {
      ipv4 {
        address = "10.50.1.30/24"
      }
    }
  }
}
