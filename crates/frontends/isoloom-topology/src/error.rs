//! What can be wrong with a drawing, said so the user knows what to fix: one enum for the crate.

use isoloom_spec::syntax::Problems;

use crate::cables::LINK_BLOCK;

/// The crate's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a drawing doesn't compile.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A cable names a node the project doesn't have.
    #[error("link {link} refers to node `{node}`, which the project doesn't have")]
    UnknownNode {
        /// The cable's index.
        link: usize,
        /// The node it names.
        node: String,
    },
    /// A cable runs from a node to itself.
    #[error("link {link} cables `{node}` to itself")]
    SelfLink {
        /// The cable's index.
        link: usize,
        /// The node.
        node: String,
    },
    /// Two cables share a port.
    #[error("port `{port}` of `{node}` already has a cable")]
    PortInUse {
        /// The node.
        node: String,
        /// Its port.
        port: String,
    },
    /// More cables than the cables' block holds.
    #[error("{0} cables don't fit in {LINK_BLOCK}")]
    TooManyLinks(usize),
    /// A node is on a LAN the project doesn't have.
    #[error("`{node}` is on LAN `{lan}`, which the project doesn't have")]
    UnknownLan {
        /// The node.
        node: String,
        /// The LAN it names.
        lan: String,
    },
    /// A LAN's or a switch's name starts with `link-`, which the cables' networks take
    /// (`link-<n>`): a switch's name is the name of the network its VLANs nest under.
    #[error("`{0}` starts with `link-`, which the cables' networks take; rename it")]
    ReservedName(String),
    /// A LAN's CIDR isn't a block Isoloom gives machines addresses on: its network's own address
    /// and a /24 to a /29.
    #[error("LAN `{lan}`: `{cidr}` isn't a block like 10.10.1.0/24 (the network's own address, a /24 to a /29)")]
    BadCidr {
        /// The LAN.
        lan: String,
        /// Its block as drawn.
        cidr: String,
    },
    /// Two nodes ask for the same address on a LAN.
    #[error("on LAN `{lan}`, `{first}` and `{second}` both ask for .{octet}")]
    AddressTaken {
        /// The LAN.
        lan: String,
        /// The address's last octet.
        octet: u8,
        /// The node that asked first.
        first: String,
        /// The other one.
        second: String,
    },
    /// A node asks for an address Isoloom gives no machine on a LAN: outside its block, or one it
    /// keeps (the network's own, the gateway, the two last usable ones, the broadcast).
    #[error("on LAN `{lan}` ({cidr}), `{node}` asks for .{octet}, which no machine can take there")]
    AddressOutside {
        /// The LAN.
        lan: String,
        /// The node.
        node: String,
        /// The address's last octet.
        octet: u8,
        /// The LAN's block.
        cidr: String,
    },
    /// A Cisco router asks for an address on a LAN it is the gateway of: there it is at the
    /// gateway's address, which no machine asks for.
    #[error("on LAN `{lan}`, `{node}` is the gateway (the first Cisco router on it), at .{gateway}: leave its address there unset rather than .{octet}")]
    GatewayAddress {
        /// The LAN.
        lan: String,
        /// The router.
        node: String,
        /// The address it asks for.
        octet: u8,
        /// The gateway's last octet, where it is.
        gateway: u8,
    },
    /// A LAN has more machines than free addresses.
    #[error("LAN `{lan}` ({cidr}) has no free address left; use a larger block")]
    LanFull {
        /// The LAN.
        lan: String,
        /// Its block.
        cidr: String,
    },
    /// A switch has the name of a LAN: its VLANs nest under a LAN of that name in the spec.
    #[error("switch `{switch}` has the name of a LAN; rename one of them")]
    SwitchNameTaken {
        /// The switch.
        switch: String,
    },
    /// A VLAN has a `tc`: Isoloom shapes a LAN's or a cable's traffic, never one VLAN's.
    #[error("VLAN `{lan}` has a `tc`, which Isoloom gives only a whole LAN or a cable; remove it")]
    VlanTc {
        /// The VLAN's LAN.
        lan: String,
    },
    /// A port name the compiler made breaks Isoloom's rule (a bug: the names are generated).
    #[error("the name of port {port} of `{node}`: {source}")]
    PortName {
        /// The node.
        node: String,
        /// The port.
        port: u16,
        /// Why the name was refused.
        #[source]
        source: isoloom_spec::Error,
    },
    /// The spec couldn't be written as YAML.
    #[error("writing the spec: {0}")]
    Yaml(#[source] serde_yaml_ng::Error),
    /// Isoloom doesn't read the spec the compiler wrote (a bug: the compiler writes only what
    /// Isoloom's format holds).
    #[error("Isoloom doesn't read the compiled spec: {}", lines(.0))]
    Unreadable(Problems),
    /// Two LANs are the same VLAN of the same switch: the spec holds one block per VLAN id.
    #[error("LANs `{first}` and `{second}` are both VLAN {vlan} of switch `{switch}`; give one another VLAN id")]
    VlanTaken {
        /// The switch.
        switch: String,
        /// The VLAN id.
        vlan: u16,
        /// The LAN drawn first.
        first: String,
        /// The other one.
        second: String,
    },
    /// A LAN names a switch but no VLAN id, or a VLAN id but no switch: a VLAN needs both, a plain
    /// LAN neither.
    #[error("LAN `{lan}` has a {given} but no {missing}: a VLAN needs both, a plain LAN neither")]
    VlanIncomplete {
        /// The LAN.
        lan: String,
        /// What it gives: `switch` or `vlan`.
        given: &'static str,
        /// What it lacks.
        missing: &'static str,
    },
    /// A plain LAN names a `device`, which only a switch's VLANs have (the switch's machine).
    #[error("LAN `{lan}` names a device, which only a switch's VLAN takes; remove it or give the LAN a switch and a VLAN id")]
    DeviceOffSwitch {
        /// The LAN.
        lan: String,
    },
    /// A machine's provisioning step isn't a file of the project as Isoloom reads one: an
    /// absolute path, or one with a `..` that leaves the project.
    #[error("provisioning step `{path}` of `{node}`: {source}")]
    StepPath {
        /// The machine.
        node: String,
        /// The step's path as drawn.
        path: String,
        /// Why Isoloom refuses it.
        #[source]
        source: Box<isoloom_spec::Error>,
    },
}

/// The problems, on one line, separated by semicolons.
fn lines(problems: &Problems) -> String {
    problems.iter().map(isoloom_spec::syntax::Problem::line).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_isoloom_does_not_read_says_each_problem() {
        let problems = isoloom_spec::syntax::parse("version: 1\nname: x\nmachines:\n  a:\n    networks: { lan: x }\n  b:\n    color: red\n").unwrap_err();
        assert!(problems.len() >= 2, "{problems:?}");
        let said = Error::Unreadable(problems.clone()).to_string();
        let expected: Vec<String> = problems.iter().map(isoloom_spec::syntax::Problem::line).collect();
        assert_eq!(said, format!("Isoloom doesn't read the compiled spec: {}", expected.join("; ")));
    }

    #[test]
    fn a_message_says_its_cause() {
        let refused = "".parse::<isoloom_spec::syntax::PortName>().unwrap_err();
        let cause = refused.to_string();
        let port = Error::PortName {
            node: "web".into(),
            port: 80,
            source: refused,
        };
        assert_eq!(port.to_string(), format!("the name of port 80 of `web`: {cause}"));
        let yaml = serde_yaml_ng::from_str::<u8>("[").unwrap_err();
        assert_eq!(
            Error::Yaml(yaml).to_string(),
            format!("writing the spec: {}", serde_yaml_ng::from_str::<u8>("[").unwrap_err())
        );
    }
}
