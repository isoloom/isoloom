# Written the way GOAD's AWS provider is: a map of machines, one instance and one network
# interface each (for_each), addresses on the interfaces, in one subnet.
variable "vm_config" {
  default = {
    "dc01" = { ami = "ami-0windows2019", instance_type = "t2.medium", private_ip_address = "192.168.56.10" }
    "srv02" = { ami = "ami-0windows2019", instance_type = "t3.large", private_ip_address = "192.168.56.22" }
  }
}

locals {
  cidr = "192.168.56.0/24"
}

resource "aws_vpc" "lab" {
  cidr_block = "192.168.0.0/16"
}

resource "aws_subnet" "lab_private" {
  vpc_id     = aws_vpc.lab.id
  cidr_block = local.cidr
}

resource "aws_network_interface" "nic" {
  for_each    = var.vm_config
  subnet_id   = aws_subnet.lab_private.id
  private_ips = [each.value.private_ip_address]
}

resource "aws_instance" "vm" {
  for_each      = var.vm_config
  ami           = each.value.ami
  instance_type = each.value.instance_type
  tags          = { OS = "windows" }
  network_interface {
    network_interface_id = aws_network_interface.nic[each.key].id
    device_index         = 0
  }
}

resource "aws_s3_bucket" "logs" {
  bucket = "lab-logs"
}
