//! Scala Native libraries from Maven, fetched by Coursier with what they depend on.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The Maven coordinate of `dep`: `g::a::v` and `g::a:v` name the Scala Native library
/// `g:a_native0.5_3:v`; `g:a:v` stays.
pub fn coordinate(dep: &str) -> Result<String, String> {
    let parts: Vec<&str> = dep.split(':').collect();
    let named = |ps: &[&str]| ps.iter().all(|p| !p.is_empty());
    match parts[..] {
        [g, "", a, "", v] | [g, "", a, v] if named(&[g, a, v]) => Ok(format!("{g}:{a}_native0.5_3:{v}")),
        [g, a, v] if named(&[g, a, v]) => Ok(dep.to_owned()),
        _ => Err(format!(
            "{dep}: expected group::artifact::version, group::artifact:version or group:artifact:version"
        )),
    }
}

/// The jars of `deps` and of what they depend on, without the Scala and Scala Native libraries:
/// those Coursier listed before for the same coordinates, if all are still there and none
/// names a version range, `latest` or a snapshot, else those Coursier fetches now.
pub fn fetch(deps: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut coordinates = deps.iter().map(|d| coordinate(d)).collect::<Result<Vec<_>, _>>()?;
    coordinates.sort();
    coordinates.dedup();
    if coordinates.is_empty() {
        return Ok(Vec::new());
    }
    let mut h = DefaultHasher::new();
    coordinates.hash(&mut h);
    let list = crate::cache_dir().join("deps").join(format!("{:016x}", h.finish()));
    let fixed = !coordinates
        .iter()
        .any(|c| c.contains(['[', '(', ',', '+']) || c.contains("latest") || c.contains("SNAPSHOT"));
    if fixed && let Some(jars) = listed(&list) {
        return Ok(jars);
    }
    eprintln!("fetching {}", coordinates.join(" "));
    let out = run_coursier(&coordinates)?;
    if !out.status.success() {
        return Err(format!("Coursier could not fetch {}", coordinates.join(" ")));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let jars: Vec<PathBuf> = text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
    for jar in jars.iter().filter(|j| !has_nir(j)) {
        eprintln!("warning: {} holds no NIR; only Scala Native libraries link", jar.display());
    }
    let _ =
        std::fs::create_dir_all(list.parent().unwrap_or(&list)).and_then(|()| std::fs::write(&list, text));
    Ok(jars)
}

/// The jars listed in file `list`, if it exists and they all do.
fn listed(list: &Path) -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string(list).ok()?;
    let jars: Vec<PathBuf> = text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
    jars.iter().all(|j| j.exists()).then_some(jars)
}

/// `coursier fetch` of `coordinates`, by `$SCIVOLO_COURSIER`, else `coursier`, else `cs`; its
/// progress and errors go to stderr.
fn run_coursier(coordinates: &[String]) -> Result<Output, String> {
    let chosen = std::env::var_os("SCIVOLO_COURSIER");
    let candidates = match &chosen {
        Some(c) => vec![c.clone()],
        None => vec!["coursier".into(), "cs".into()],
    };
    for cs in candidates {
        let run = Command::new(&cs)
            .args(["fetch", "--exclude", "org.scala-lang:*", "--exclude", "org.scala-native:*"])
            .args(coordinates)
            .stderr(Stdio::inherit())
            .output();
        match run {
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", cs.to_string_lossy())),
            Ok(out) => return Ok(out),
        }
    }
    Err("Coursier not found (tried coursier and cs); set SCIVOLO_COURSIER to its path".into())
}

/// Whether `jar` holds a `.nir` file, by the names in its directory.
fn has_nir(jar: &Path) -> bool {
    std::fs::read(jar).is_ok_and(|bytes| bytes.windows(4).any(|w| w == b".nir"))
}

#[cfg(test)]
mod tests {
    use super::coordinate;

    #[test]
    fn scala_coordinates_name_native_artifacts() {
        let native = Ok("com.lihaoyi:geny_native0.5_3:1.1.0".to_owned());
        assert_eq!(coordinate("com.lihaoyi::geny::1.1.0"), native);
        assert_eq!(coordinate("com.lihaoyi::geny:1.1.0"), native);
        assert_eq!(coordinate("org.x:y:1"), Ok("org.x:y:1".into()));
        assert!(coordinate("geny").is_err() && coordinate("a::b").is_err() && coordinate("a::b:").is_err());
    }
}
