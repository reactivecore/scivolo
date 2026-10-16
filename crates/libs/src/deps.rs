//! Maven dependencies fetched by Coursier as Scala Native jars, as `scripts/repl-deps.sh` does.

use std::path::PathBuf;
use std::process::Command;

/// The Maven coordinate of `dep`: `g::a::v` becomes `g:a_native0.5_3:v`, `g:a:v` stays.
pub fn coordinate(dep: &str) -> Result<String, String> {
    let parts: Vec<&str> = dep.split(':').collect();
    match parts[..] {
        [g, "", a, "", v] if [g, a, v].iter().all(|p| !p.is_empty()) => {
            Ok(format!("{g}:{a}_native0.5_3:{v}"))
        }
        [g, a, v] if [g, a, v].iter().all(|p| !p.is_empty()) => Ok(dep.to_owned()),
        _ => Err(format!("{dep}: expected group::artifact::version or group:artifact:version")),
    }
}

/// The jars of `deps` and of what they depend on, but the Scala and Scala Native libraries,
/// fetched by `$SCALAI_COURSIER` (default `coursier`).
pub fn fetch(deps: &[String]) -> Result<Vec<PathBuf>, String> {
    if deps.is_empty() {
        return Ok(Vec::new());
    }
    let coordinates = deps.iter().map(|d| coordinate(d)).collect::<Result<Vec<_>, _>>()?;
    let cs = std::env::var_os("SCALAI_COURSIER").unwrap_or_else(|| "coursier".into());
    let out = Command::new(&cs)
        .args(["fetch", "--exclude", "org.scala-lang:*", "--exclude", "org.scala-native:*"])
        .args(&coordinates)
        .output()
        .map_err(|e| format!("{}: {e}", cs.to_string_lossy()))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("fetching {}: {}", deps.join(" "), why.trim()));
    }
    let jars = String::from_utf8_lossy(&out.stdout);
    Ok(jars.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::coordinate;

    #[test]
    fn scala_coordinates_name_native_artifacts() {
        assert_eq!(coordinate("com.lihaoyi::geny::1.1.0"), Ok("com.lihaoyi:geny_native0.5_3:1.1.0".into()));
        assert_eq!(coordinate("org.x:y:1"), Ok("org.x:y:1".into()));
        assert!(coordinate("geny").is_err() && coordinate("a::b").is_err());
    }
}
