//! Why the engine can't give a plan or a download: one enum for the crate. Its message is what the
//! caller shows (a `JsError`'s `message`, or a plan's `error`), so a variant with a cause says it
//! too.

/// The crate's result.
pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a topology gives no plan or no download.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The topology isn't a [`Topology`](isoloom_topology::Topology)'s JSON.
    #[error("the topology is malformed: {0}")]
    Malformed(#[source] serde_json::Error),
    /// The drawing doesn't compile.
    #[error(transparent)]
    Compile(#[from] isoloom_topology::Error),
    /// Isoloom finds problems in the spec: nothing is generated until they're fixed.
    #[error("fix the spec first: {}", .0.join("; "))]
    Problems(Vec<String>),
    /// No target generates any file for the project.
    #[error("no target generates any file for this project: the plan's `skipped` says why")]
    NothingGenerated,
    /// The project generates nothing for that target.
    #[error("this project has no files for `{0}`")]
    NoFiles(String),
    /// The zip couldn't be written.
    #[error("writing the zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// A file couldn't be added to the zip.
    #[error("writing the zip: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a JavaScript caller reads (a `JsError`'s `message` is the error's text) says the cause.
    #[test]
    fn every_message_says_its_cause() {
        let io = Error::Io(std::io::Error::other("disk full"));
        assert_eq!(io.to_string(), "writing the zip: disk full");
        let zip = Error::Zip(zip::result::ZipError::FileNotFound);
        assert_eq!(zip.to_string(), "writing the zip: specified file not found in archive");
        let refused = "".parse::<isoloom_spec::syntax::PortName>().unwrap_err();
        let cause = refused.to_string();
        let port = Error::Compile(isoloom_topology::Error::PortName {
            node: "web".into(),
            port: 80,
            source: refused,
        });
        assert_eq!(port.to_string(), format!("the name of port 80 of `web`: {cause}"));
    }
}
