# What the mocked Oracle Cloud provider answers where a random value wouldn't do: the
# compartment's availability domains and the Ubuntu images (an instance takes the first of each).
mock_data "oci_identity_availability_domains" {
  defaults = {
    availability_domains = [{ name = "Uocm:EU-PARIS-1-AD-1", compartment_id = "ocid1.compartment.oc1..test", id = "ocid1.availabilitydomain.oc1..test" }]
  }
}

mock_data "oci_core_images" {
  defaults = {
    images = [{ id = "ocid1.image.oc1..ubuntu2404" }]
  }
}
