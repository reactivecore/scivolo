//! The version the programs report.

/// The workspace version, followed by `+g` and the commit for a build that is not of the commit
/// tagged `v<version>`, e.g. `0.2.3` or `0.2.3+g1a2b3c4d5`.
pub const VERSION: &str = env!("SCIVOLO_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_the_package_version_with_an_optional_commit() {
        let rest = VERSION.strip_prefix(env!("CARGO_PKG_VERSION")).expect(VERSION);
        let ok = match rest.strip_prefix("+g") {
            Some(commit) => !commit.is_empty() && commit.chars().all(|c| c.is_ascii_hexdigit()),
            None => rest.is_empty(),
        };
        assert!(ok, "{VERSION}");
    }
}
