//! A machine's own cloud image (`vm.image.<cloud>`): each cloud reads its own field and boots it
//! instead of the image it would look up for the machine's OS. The OS still says what the image
//! is (Linux or Windows, and the account the cloud's images log in with).

use isoloom_plan::MachinePlan;
use isoloom_spec::model::{Machine, VmImage};

/// Which field of `vm.image` a cloud reads.
pub type Field = fn(&VmImage) -> Option<&String>;

/// AWS: an AMI id.
pub fn aws(i: &VmImage) -> Option<&String> {
    i.aws.as_ref()
}

/// Azure: an image's resource id.
pub fn azure(i: &VmImage) -> Option<&String> {
    i.azure.as_ref()
}

/// Google Cloud: an image's path.
pub fn gcp(i: &VmImage) -> Option<&String> {
    i.gcp.as_ref()
}

/// DigitalOcean: a slug or an image id.
pub fn digitalocean(i: &VmImage) -> Option<&String> {
    i.digitalocean.as_ref()
}

/// Linode: an image id.
pub fn linode(i: &VmImage) -> Option<&String> {
    i.linode.as_ref()
}

/// Oracle Cloud: an image OCID.
pub fn oci(i: &VmImage) -> Option<&String> {
    i.oci.as_ref()
}

/// The machine's own image on the cloud whose field is `field`, when the spec gives one.
pub fn of(mp: &MachinePlan, field: Field) -> Option<&str> {
    mp.vm_image.as_ref().and_then(field).map(String::as_str)
}

/// [`of`], for a machine of the spec (the refusals that read it).
pub fn of_machine(m: &Machine, field: Field) -> Option<&str> {
    m.vm.as_ref()?.image.as_ref().and_then(field).map(String::as_str)
}
