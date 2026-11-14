//! Isoloom's topology compiler: a project as drawn in a topology editor (a [`Topology`]: machines, the LANs drawn
//! around them, and the cables between their ports) becomes an Isoloom spec, in the format Isoloom
//! reads (`isoloom.yml`, version 1).
//!
//! - **Machines.** A container node takes every form Isoloom has of its image: a distro image is
//!   its OS (`os`), a service image the catalogue has is that service (`os` + `service:`), both a
//!   container and a VM; any other image is a custom app from it (`app.image`, a container only,
//!   which a VM target refuses), kept running idle with its own `command` when the image runs
//!   nothing of its own. A Cisco node is an appliance (`app.appliance`). A VM node is its OS (`os`), set up by its provisioning steps
//!   (`app.steps`), or the OS alone: each target decides how to deliver it.
//! - **Ports** are named (`ssh`, `http`, `port-8000`, `dns-udp`), each with its `protocol`.
//! - **Networks.** A LAN (an area drawn around machines, a VLAN in practice) is an Isoloom network
//!   with its own block; each machine on it gets an address there, the next free one unless it
//!   asks for one. A cable between two machines is its own point-to-point network (a /29 cut from
//!   `10.200.0.0/16`, the two ends on its second and third host addresses). A switch's VLANs nest
//!   under a network named after the switch.
//! - **Routing.** An automatic router lets the networks it joins reach each other (`reach`, both
//!   ways); a Cisco router is its LANs' gateway.
//!
//! Every compiled spec is read back by Isoloom's parser before it is returned, so what the
//! compiler hands out is a spec Isoloom reads. Whether the project is valid as a whole (an unattached
//! machine, overlapping blocks) is Isoloom's validation to say.
//!
//! A Windows machine's own steps are PowerShell scripts: the Ansible playbooks drawn on one run
//! from Isoloom's controller (`provision:`), as Isoloom's own Windows examples do.
//!
//! The project's own files come with the spec ([`Compiled::into_parts`], as [`ProjectFile`]s): the
//! appliances' configurations, and a starter for each provisioning step to edit, so a downloaded
//! project runs as is.
//!
//! The modules follow [`compile`]: the drawing as read (`topology`); its `machines`, the
//! `forms` of their containers and their `ports`; the playbooks Isoloom's `controller` runs; the
//! `lans` and their addresses, the `cables`, the `routers` and the switches' `vlans`; the spec as
//! it is built (`draft`) and written (`document`). `project` and `starters` are the project's own
//! files; `error` is what a drawing can get wrong.
//!
//! ```
//! let topology: isoloom_topology::Topology = serde_json::from_str(
//!     r#"{
//!       "name": "pair",
//!       "networks": { "office": { "cidr": "10.10.1.0/24" } },
//!       "nodes": {
//!         "web":  { "kind": { "container": { "image": "nginx:1.27-alpine" } },
//!                   "services": [{ "port": 80, "protocol": "http" }], "networks": { "office": null } },
//!         "user": { "kind": { "vm": { "os": "debian@12" } }, "access": true, "networks": { "office": null } }
//!       }
//!     }"#,
//! )?;
//! let compiled = isoloom_topology::compile(&topology)?;
//! let (yaml, spec, _files) = compiled.into_parts();
//! assert!(yaml.contains("app:\n      image: nginx:1.27-alpine"));
//! assert!(yaml.contains("ports:\n      http:\n        port: 80\n        protocol: http"));
//! assert_eq!(spec.machines["user"].os.map(|os| os.to_string()).as_deref(), Some("debian@12"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod cables;
mod compiled;
mod controller;
mod document;
mod draft;
mod error;
mod forms;
mod lans;
mod machines;
mod ports;
mod project;
mod routers;
mod starters;
mod topology;
mod vlans;

pub use compiled::{Compiled, compile};
pub use error::{Error, Result};
pub use project::ProjectFile;
pub use topology::{Endpoint, Kind, Lan, Link, Node, Service, Topology};
