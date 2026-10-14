//! The utility providers the modules use beside each platform's own (keys, passwords, files,
//! waits, Terraform's built-in `terraform_data`), and the ones a module may need next
//! (cloud-init documents, HTTP checks, triggers). They act on Terraform's side only, so what
//! Isoloom doesn't write of them is tooling.

use super::support::CREDENTIALS;
use super::table::{Provider, Rules, Table};
use crate::Support::{self, Emitted, Tooling};

/// What the modules write of them.
const UTILITY_WRITTEN: Support = Emitted {
    from: "(written by the generators: the environment's key and passwords, the project's file list, Windows restarts, and the steps run on each machine)",
};
/// Everything else: values, files and waits on Terraform's side.
const UTILITY: Support = Tooling {
    note: "a value, file or wait on Terraform's side: no change in how the environment behaves",
};

macro_rules! provider {
    ($name:literal) => {
        Provider {
            name: $name,
            types: include_str!(concat!("../../coverage/terraform/", $name, ".txt")),
            schema: include_str!(concat!("../../coverage/terraform/", $name, "-schema.txt")),
        }
    };
}

/// The utility providers: every resource type and data source, with their arguments.
pub const TABLE: Table = Table {
    name: "Terraform: utility providers",
    file: "every Terraform module under .isoloom/",
    source: "Terraform's built-in provider, hashicorp/random 3.9.1, hashicorp/tls 4.4.1, hashicorp/local 2.9.1, hashicorp/time 0.14.2, hashicorp/null 3.3.2, hashicorp/cloudinit 2.4.1 and hashicorp/http 3.6.2 (their whole schemas)",
    providers: &[
        provider!("terraform"),
        provider!("random"),
        provider!("tls"),
        provider!("local"),
        provider!("time"),
        provider!("null"),
        provider!("cloudinit"),
        provider!("http"),
    ],
    prefix: "",
    legacy: None,
    generated: UTILITY_WRITTEN,
    types: TYPES,
    written: WRITTEN,
    settings: SETTINGS,
    arguments: &[],
    every_argument: Some(UTILITY),
    collapse: false,
};

/// Every resource type and data source, by provider.
const TYPES: Rules = &[(UTILITY, "terraform_* random_* tls_* local_* time_* null_* cloudinit_* http")];

/// What the generators write, from the committed examples' modules.
const WRITTEN: &[(&str, &str)] = &[
    ("types", "local_file random_password terraform_data time_sleep tls_private_key"),
    ("local_file", "content filename"),
    ("random_password", "length min_lower min_numeric min_upper special"),
    ("terraform_data", "input triggers_replace"),
    ("time_sleep", "create_duration"),
    ("tls_private_key", "algorithm"),
];

/// The providers' settings: only the TLS provider's proxy.
const SETTINGS: Rules = &[(CREDENTIALS, "proxy.*")];
