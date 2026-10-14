//! Every Dockerfile instruction, each instruction's flags and the parser directives (BuildKit's
//! Dockerfile frontend, listed in `coverage/dockerfile.txt` by `scripts/refresh-dockerfile.py`),
//! and what Isoloom does with each. Isoloom writes a Dockerfile for the emulators it builds itself
//! (Dynamips, under `.isoloom/docker/appliances/<machine>/build/`); a machine's own image is the
//! user's Dockerfile (`machines.*.docker.build`), built as is.
//!
//! This is where the Compose table's "in image" rows land: a machine's command, user, working
//! directory, files and stop signal are its image's, set in its Dockerfile (or, on a VM, its
//! provisioning).

use super::{
    Format, Support,
    Support::{Emitted, Equivalent, InImage, Tooling},
};

const IMAGE: Support = InImage {
    note: "the machine's own Dockerfile (`machines.*.docker.build`), built as is; on a VM, its provisioning (`vm.provision`)",
};
const BUILD: Support = Tooling {
    note: "how the image is built (caching, layers, sandboxing), not what the machine is",
};
const METADATA: Support = Tooling {
    note: "image metadata: no effect on the machine",
};
const PROBE: Support = Equivalent {
    via: "machines.*.services (Isoloom probes every port, in Compose's healthcheck)",
};

/// The Dockerfile table.
pub fn format() -> Format {
    Format {
        name: "Dockerfile",
        file: ".isoloom/docker/appliances/<machine>/build/Dockerfile",
        source: "BuildKit's Dockerfile frontend dockerfile/1.28.0 (crates/isoloom-coverage/coverage/dockerfile.txt)",
        rows: ROWS.iter().map(|(k, s)| ((*k).to_string(), *s)).collect(),
        collapse_not_portable: false,
    }
}

/// Every instruction and flag, in the list's order, then the directives. Data, not code: one row
/// an entry.
const ROWS: &[(&str, Support)] = &[
    ("ADD", IMAGE),
    ("ADD --checksum", BUILD),
    ("ADD --chmod", IMAGE),
    ("ADD --chown", IMAGE),
    ("ADD --exclude", BUILD),
    ("ADD --keep-git-dir", BUILD),
    ("ADD --link", BUILD),
    ("ADD --unpack", BUILD),
    (
        "ARG",
        Equivalent {
            via: "machines.*.docker.args (Compose's build arguments)",
        },
    ),
    ("CMD", IMAGE),
    ("COPY", IMAGE),
    ("COPY --chmod", IMAGE),
    ("COPY --chown", IMAGE),
    ("COPY --exclude", BUILD),
    ("COPY --from", BUILD),
    ("COPY --link", BUILD),
    ("COPY --parents", BUILD),
    ("ENTRYPOINT", IMAGE),
    (
        "ENV",
        Equivalent {
            via: "machines.*.environment (set on the machine itself)",
        },
    ),
    (
        "EXPOSE",
        Equivalent {
            via: "machines.*.services (every port is reachable on the machine's networks)",
        },
    ),
    (
        "FROM",
        Emitted {
            from: "machines.*.docker.appliance (the emulator's base image, through the registry mirrors)",
        },
    ),
    (
        "FROM --platform",
        Equivalent {
            via: "machines.*.arch (Compose's platform for the build)",
        },
    ),
    ("HEALTHCHECK", PROBE),
    ("HEALTHCHECK --interval", PROBE),
    ("HEALTHCHECK --retries", PROBE),
    ("HEALTHCHECK --start-interval", PROBE),
    ("HEALTHCHECK --start-period", PROBE),
    ("HEALTHCHECK --timeout", PROBE),
    ("LABEL", METADATA),
    ("MAINTAINER", METADATA),
    (
        "ONBUILD",
        Tooling {
            note: "runs in images built from this one, not in this machine",
        },
    ),
    (
        "RUN",
        Emitted {
            from: "machines.*.docker.appliance (installs the emulator)",
        },
    ),
    ("RUN --device", BUILD),
    ("RUN --mount", BUILD),
    ("RUN --network", BUILD),
    ("RUN --security", BUILD),
    ("SHELL", BUILD),
    ("STOPSIGNAL", IMAGE),
    ("USER", IMAGE),
    ("VOLUME", Equivalent { via: "machines.*.volumes" }),
    ("WORKDIR", IMAGE),
    ("directive check", BUILD),
    ("directive escape", BUILD),
    ("directive syntax", BUILD),
];
