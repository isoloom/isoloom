//! The Docker Compose backend: the Compose file the `docker` and `hosted` targets run (and the
//! Docker-on-a-VM targets carry), with the network appliances.

pub mod appliances;
pub mod docker;

pub use docker::{ENVIRONMENT_LABEL, MANAGED_LABEL, StartPlan, exec_runner, leaf_jobs, start_commands, start_plan};
use isoloom_plan::checks;
use isoloom_render::{GenerateError, GeneratedFile, OUTPUT_DIR, address, address_for, common_unsupported, constraints, header, host_address, router, trunks};
use isoloom_spec::{model, validate, vlans};
