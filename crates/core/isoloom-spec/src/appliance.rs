//! What the spec needs to know of the network appliances (`docker.appliance`).

use crate::model::Appliance;

/// The appliances' management network, outside the environment.
pub const MGMT_CIDR: &str = "10.255.255.0/24";

/// Whether the appliance is a switch (its interfaces take no address).
pub fn is_switch(kind: Appliance) -> bool {
    matches!(kind, Appliance::CiscoIolL2 | Appliance::CiscoViosL2)
}
