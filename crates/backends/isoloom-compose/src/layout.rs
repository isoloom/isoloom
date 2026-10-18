//! Where the Compose file sits in the project (`.isoloom/docker/`), and the project's paths as
//! the Compose file sees them.

/// The target's folder, under `.isoloom/`.
pub(crate) const DIR: &str = "docker";
/// From `.isoloom/docker/` back to the project folder.
pub(crate) const ROOT: &str = "../..";

/// `path` (from the project folder) as seen from the folder `from` (also from the project).
pub(crate) fn relative_to(from: &str, path: &str) -> String {
    let parts = |p: &str| -> Vec<String> { p.split('/').filter(|c| !c.is_empty() && *c != ".").map(String::from).collect() };
    let (from, path) = (parts(from), parts(path));
    let common = from.iter().zip(&path).take_while(|(a, b)| a == b).count();
    let up = std::iter::repeat_n("..".to_owned(), from.len().saturating_sub(common));
    up.chain(path.into_iter().skip(common)).collect::<Vec<_>>().join("/")
}

/// The last component of a `/`-separated path (a script's name in the container).
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_path_relative_to_a_folder() {
        assert_eq!(relative_to("app", "build/shop/Dockerfile"), "../build/shop/Dockerfile");
        assert_eq!(relative_to("build/web", "build/web/Dockerfile.dev"), "Dockerfile.dev");
        assert_eq!(relative_to(".", "docker/Dockerfile"), "docker/Dockerfile");
        assert_eq!(relative_to("./src/", "Dockerfile"), "../Dockerfile");
    }

    #[test]
    fn a_file_name_is_the_last_component() {
        assert_eq!(file_name("checks/seed.sh"), "seed.sh");
        assert_eq!(file_name("seed.sh"), "seed.sh");
    }

    /// A folder or file name: no `/`, never `.` or `..`.
    fn component() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_.-]{0,6}"
    }

    proptest! {
        #[test]
        fn going_up_then_down_lands_on_the_path(from in prop::collection::vec(component(), 0..4), path in prop::collection::vec(component(), 1..4)) {
            let relative = relative_to(&from.join("/"), &path.join("/"));
            // Resolved from `from`, the relative path names `path` again.
            let mut at = from.clone();
            for part in relative.split('/').filter(|p| !p.is_empty()) {
                if part == ".." {
                    prop_assert!(at.pop().is_some(), "{relative} climbs above the project");
                } else {
                    at.push(part.to_owned());
                }
            }
            prop_assert_eq!(at, path);
        }
    }
}
