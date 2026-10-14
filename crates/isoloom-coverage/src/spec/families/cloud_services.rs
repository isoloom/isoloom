//! The cloud-services scripts (`cloud-services`): fields not listed mean nothing to cloud services.

use crate::spec::status::Status::{Done, NotApplicable};
use crate::spec::status::{PROBE, Status};

/// The cloud services output's cell for each field it uses.
pub const CELLS: &[(&str, Status)] = &[
    (
        "name",
        Done {
            how: "the scripts' header and the deployment's tag",
            proof: "cloud-bucket",
        },
    ),
    (
        "inputs",
        Done {
            how: "launch-time values written to inputs.tfvars.json",
            proof: PROBE,
        },
    ),
    (
        "cloud.provider",
        Done {
            how: "the account the scripts use and the host readiness check",
            proof: "cloud-bucket",
        },
    ),
    (
        "cloud.terraform",
        Done {
            how: "the module, copied and applied by up.sh, destroyed by down.sh",
            proof: "cloud-bucket",
        },
    ),
    (
        "cloud.vars",
        Done {
            how: "terraform.tfvars.json",
            proof: "cloud-bucket",
        },
    ),
    (
        "cloud.outputs",
        Done {
            how: "read by outputs.sh; they fill the checks and the message",
            proof: "cloud-bucket",
        },
    ),
    (
        "cloud.hourly_usd",
        Done {
            how: "an estimate: `isoloom run` says it, the snapshot carries it",
            proof: "cloud-bucket",
        },
    ),
    (
        "cloud.before",
        Done {
            how: "host steps in up.sh, before the apply",
            proof: PROBE,
        },
    ),
    (
        "cloud.env",
        Done {
            how: "exported by every script",
            proof: PROBE,
        },
    ),
    (
        "cloud.apply_retries",
        Done {
            how: "up.sh applies again after a failed apply",
            proof: PROBE,
        },
    ),
    (
        "cloud.requires",
        Done {
            how: "host tools `doctor` and `run` check before the apply",
            proof: PROBE,
        },
    ),
    (
        "provision[].ansible",
        NotApplicable {
            why: "cloud services aren't machines: nothing to provision",
        },
    ),
    (
        "checks[]",
        Done {
            how: "one runner, run from this computer by `isoloom test cloud-services`",
            proof: PROBE,
        },
    ),
    (
        "checks[].name",
        Done {
            how: "the runner's PASS/FAIL line",
            proof: "cloud-bucket",
        },
    ),
    (
        "checks[].http",
        Done {
            how: "curl (else wget, else bash) from here, retried for `wait`",
            proof: "cloud-bucket",
        },
    ),
    (
        "checks[].tcp",
        Done {
            how: "nc (else bash) from here",
            proof: PROBE,
        },
    ),
    ("checks[].method", Done { how: "curl -X", proof: PROBE }),
    ("checks[].headers", Done { how: "curl -H", proof: PROBE }),
    (
        "checks[].body",
        Done {
            how: "curl --data-binary",
            proof: PROBE,
        },
    ),
    (
        "checks[].contains",
        Done {
            how: "the response body or the first 5s read from the connection, searched for the text",
            proof: "cloud-bucket",
        },
    ),
    (
        "checks[].follow",
        Done {
            how: "redirects followed, the final status compared",
            proof: PROBE,
        },
    ),
    (
        "checks[].send",
        Done {
            how: "a line written to the connection before reading",
            proof: PROBE,
        },
    ),
    (
        "checks[].exec",
        NotApplicable {
            why: "no machine to run it in",
        },
    ),
    (
        "checks[].script",
        Done {
            how: "sh from the project folder, each output as `ISOLOOM_OUTPUT_<NAME>`",
            proof: PROBE,
        },
    ),
    (
        "checks[].expect",
        Done {
            how: "a status code, `any` or `blocked` (http); `open` or `blocked` (tcp)",
            proof: PROBE,
        },
    ),
    (
        "checks[].wait",
        Done {
            how: "retried every 2s until it passes or the time is up",
            proof: "cloud-bucket",
        },
    ),
    (
        "message",
        Done {
            how: "printed by `run` and `message`, with the outputs filled in",
            proof: "cloud-bucket",
        },
    ),
];
