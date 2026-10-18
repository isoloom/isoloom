//! What the spec needs to know of the network appliances (`docker.appliance`).

use crate::model::Appliance;

/// The appliances' management network, outside the environment.
pub const MGMT_CIDR: &str = "10.255.255.0/24";

/// Whether the appliance is a switch (its interfaces take no address).
#[must_use]
pub fn is_switch(kind: Appliance) -> bool {
    matches!(kind, Appliance::CiscoIolL2 | Appliance::CiscoViosL2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_layer_2_images_are_switches() {
        assert!(is_switch(Appliance::CiscoIolL2) && is_switch(Appliance::CiscoViosL2));
        for router in [
            Appliance::CiscoIol,
            Appliance::CiscoVios,
            Appliance::CiscoCsr1000v,
            Appliance::CiscoC8000v,
            Appliance::CiscoDynamips,
        ] {
            assert!(!is_switch(router), "{router:?}");
        }
    }
}
