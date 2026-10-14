//! The fields every output treats alike: read by Isoloom itself, or descriptive.

use crate::spec::status::Status::{self, Core, Descriptive};

/// Each field's cell in every output that doesn't list it.
pub const CELLS: &[(&str, Status)] = &[
    (
        "version",
        Core {
            note: "selects the format version",
        },
    ),
    (
        "cloud.provider",
        Descriptive {
            note: "the cloud of a cloud-services environment (the cloud-services target, not these outputs)",
        },
    ),
    (
        "cloud.terraform",
        Descriptive {
            note: "the Terraform module the cloud-services target applies",
        },
    ),
    (
        "cloud.vars",
        Descriptive {
            note: "the module's variables (terraform.tfvars.json)",
        },
    ),
    (
        "cloud.outputs",
        Descriptive {
            note: "what checks and the message read once deployed",
        },
    ),
    (
        "cloud.hourly_usd",
        Descriptive {
            note: "an estimate: `isoloom run` says it, the snapshot carries it",
        },
    ),
    (
        "cloud.before",
        Descriptive {
            note: "host steps before the apply (up.sh)",
        },
    ),
    (
        "cloud.env",
        Descriptive {
            note: "Terraform's environment (the cloud-services scripts)",
        },
    ),
    (
        "cloud.apply_retries",
        Descriptive {
            note: "applies again after a failed apply (up.sh)",
        },
    ),
    (
        "cloud.requires",
        Descriptive {
            note: "host tools `doctor` and `run` check",
        },
    ),
    (
        "machines.*.ports[].name",
        Descriptive {
            note: "names the port for people and runners",
        },
    ),
    (
        "machines.*.ports[].tls",
        Descriptive {
            note: "derived checks reach it over TLS (HTTPS for an `http` service)",
        },
    ),
    (
        "machines.*.ports[].http",
        Descriptive {
            note: "tells runners they can open it in a browser",
        },
    ),
    (
        "targets",
        Core {
            note: "narrows the targets Isoloom generates",
        },
    ),
    (
        "machines.*.external.address",
        Core {
            note: "the `external` target's SSH endpoint (inventory, checks, connect)",
        },
    ),
    (
        "machines.*.external.user",
        Core {
            note: "the `external` target's SSH user",
        },
    ),
    (
        "machines.*.external.port",
        Core {
            note: "the `external` target's SSH port",
        },
    ),
    (
        "machines.*.external.key",
        Core {
            note: "the `external` target's SSH key",
        },
    ),
    (
        "message",
        Descriptive {
            note: "printed by `isoloom run` and `isoloom message`, placeholders filled from the snapshot",
        },
    ),
    (
        "machines.*.count",
        Core {
            note: "expanded into clones before generation",
        },
    ),
    (
        "common",
        Core {
            note: "folded into every machine before generation",
        },
    ),
];
