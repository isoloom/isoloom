# What the mocked AWS provider answers where a random value wouldn't do: the region's zones (a
# subnet takes the first).
mock_data "aws_availability_zones" {
  defaults = {
    names = ["eu-west-3a", "eu-west-3b", "eu-west-3c"]
  }
}
