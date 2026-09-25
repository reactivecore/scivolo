//! Locates the precompiled Scala Native libraries: in `$SCALAI_LIBS` if set, else in the
//! Coursier cache.

use std::path::PathBuf;

pub const SCALA: &str = "3.9.0";
pub const NATIVE: &str = "0.5.12";

const LIBS: [&str; 6] = ["auxlib", "javalib", "nativelib", "clib", "posixlib", "scalalib"];

/// Where each library jar may be, relative to a search root.
fn candidates(lib: &str) -> Vec<(PathBuf, String)> {
    let dir = format!("{lib}_native0.5_3");
    let version = match lib {
        "scalalib" => format!("{SCALA}%2B{NATIVE}"),
        _ => NATIVE.to_string(),
    };
    let file = format!("{dir}-{version}.jar");
    let mut out = Vec::new();
    if let Some(libs) = std::env::var_os("SCALAI_LIBS") {
        out.push((PathBuf::from(libs), file.clone()));
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let coursier: Vec<PathBuf> = std::env::var_os("COURSIER_CACHE")
        .map(PathBuf::from)
        .into_iter()
        .chain([home.join("Library/Caches/Coursier/v1"), home.join(".cache/coursier/v1")])
        .collect();
    let rel = format!("https/repo1.maven.org/maven2/org/scala-native/{dir}/{version}/{file}");
    out.extend(coursier.into_iter().map(|root| (root, rel.clone())));
    out
}

/// The library jars, in classpath order, or the first one that is missing.
pub fn jars() -> Result<Vec<PathBuf>, String> {
    LIBS.iter()
        .map(|lib| {
            candidates(lib)
                .into_iter()
                .map(|(root, rel)| root.join(rel))
                .find(|p| p.exists())
                .ok_or_else(|| format!("{lib} {NATIVE} not in $SCALAI_LIBS or the Coursier cache"))
        })
        .collect()
}
