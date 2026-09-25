//! Locates the precompiled Scala Native libraries in the Coursier cache.

use std::path::PathBuf;

pub const SCALA: &str = "3.9.0";
pub const NATIVE: &str = "0.5.12";

const LIBS: [&str; 6] = ["auxlib", "javalib", "nativelib", "clib", "posixlib", "scalalib"];

fn cache_roots() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let mut roots: Vec<PathBuf> = std::env::var_os("COURSIER_CACHE").map(PathBuf::from).into_iter().collect();
    roots.push(home.join("Library/Caches/Coursier/v1"));
    roots.push(home.join(".cache/coursier/v1"));
    roots
}

/// The library jars, in classpath order, or the first one that is missing.
pub fn jars() -> Result<Vec<PathBuf>, String> {
    LIBS.iter()
        .map(|lib| {
            let (dir, version) = match *lib {
                "scalalib" => (format!("{lib}_native0.5_3"), format!("{SCALA}%2B{NATIVE}")),
                _ => (format!("{lib}_native0.5_3"), NATIVE.to_string()),
            };
            let rel =
                format!("https/repo1.maven.org/maven2/org/scala-native/{dir}/{version}/{dir}-{version}.jar");
            cache_roots()
                .into_iter()
                .map(|root| root.join(&rel))
                .find(|p| p.exists())
                .ok_or_else(|| format!("{dir} {version} not in the Coursier cache"))
        })
        .collect()
}
