//! Why the Kubernetes files aren't rendered, or the chart's copies of the project's scripts
//! aren't made, and how a backend reports it.

use isoloom_plan::Refusal;
use isoloom_render::GenerateError;
use isoloom_render::backend::RenderError;
use isoloom_spec::model::Target;

/// Why the Kubernetes files aren't rendered, or the chart's project copies aren't made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The environment needs what Kubernetes can't do (the plan's refusal).
    #[error("{}: {refusal}", Target::Kubernetes.id())]
    Refused {
        /// The first requirement Kubernetes can't meet.
        refusal: Refusal,
    },
    /// The environment needs what Kubernetes can't do, in Kubernetes' own words (a gateway
    /// machine, GPUs it can't ask for).
    #[error("{}: {reason}", Target::Kubernetes.id())]
    Unsupported {
        /// Why, as the refusal reads.
        reason: String,
    },
    /// An object couldn't be written as YAML.
    #[error("{}: a manifest couldn't be written as YAML", Target::Kubernetes.id())]
    Unwritable {
        /// What the YAML writer reported.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// The chart's values schema couldn't be written as JSON.
    #[error("{}: the values' schema couldn't be written as JSON", Target::Kubernetes.id())]
    Schema {
        /// What the JSON writer reported.
        #[source]
        source: serde_json::Error,
    },
    /// A script the pods run exists in the project but can't be read.
    #[error("{path} (copied into the Helm chart): {source}")]
    Read {
        /// The script, from the project folder.
        path: String,
        /// Why it couldn't be read.
        #[source]
        source: std::io::Error,
    },
    /// A generated kustomization isn't one (it was edited by hand).
    #[error("{path}: not a kustomization Isoloom reads: {source}")]
    Kustomization {
        /// The kustomization, from the project folder.
        path: String,
        /// Why it couldn't be read.
        #[source]
        source: serde_yaml_ng::Error,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<Error> for RenderError {
    /// The error as every backend reports it: the plan's refusal, or what Kubernetes doesn't
    /// support, its cause in its words.
    fn from(e: Error) -> RenderError {
        let what = match e {
            Error::Refused { refusal } => {
                return RenderError::Refused {
                    target: Target::Kubernetes,
                    refusal,
                };
            }
            Error::Unsupported { reason } => reason,
            Error::Unwritable { source } => format!("a manifest doesn't serialize: {source}"),
            Error::Schema { source } => format!("the values' schema doesn't serialize: {source}"),
            read @ (Error::Read { .. } | Error::Kustomization { .. }) => read.to_string(),
        };
        RenderError::Generate(GenerateError::Unsupported {
            target: Target::Kubernetes,
            what,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_in_kubernetes_words_reads_as_the_backend_reports_it() {
        let e = Error::Unsupported {
            reason: "networks with a `gateway` machine on Kubernetes come later".to_owned(),
        };
        let reads = e.to_string();
        assert_eq!(reads, "kubernetes: networks with a `gateway` machine on Kubernetes come later");
        assert_eq!(RenderError::from(e).to_string(), reads);
    }
}
