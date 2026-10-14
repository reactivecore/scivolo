//! Locates the precompiled Scala Native libraries and the Scala standard library's TASTy: in
//! `$SCALAI_LIBS` if set, else in the Coursier cache; and maps their library image.

use libimage::{Derived, Image, Source, Spec};
use std::io;
use std::path::PathBuf;

pub const SCALA: &str = "3.9.0";
pub const NATIVE: &str = "0.5.12";

const LIBS: [&str; 6] = ["auxlib", "javalib", "nativelib", "clib", "posixlib", "scalalib"];

/// The TASTy of Scala Native's javalib (research 08 §3).
const JAVALIB_TASTY: &[u8] = include_bytes!("../../../libs/javalib-tasty-0.5.12.jar");

/// The library image of `jars`, the standard library's TASTy and javalib's: their NIR with
/// member tables, and their TASTy with its package table.
pub fn image(jars: &[PathBuf]) -> io::Result<&'static Image> {
    let mut sources: Vec<Source> = jars.iter().cloned().map(Source::File).collect();
    sources.extend(scala_library().map(Source::File));
    sources.push(Source::Bytes("javalib-tasty-0.5.12.jar", JAVALIB_TASTY));
    let keep = |name: &str| name.ends_with(".nir") || name.ends_with(".tasty");
    let packages = |names: &[&str]| tasty::package_table(names).into_bytes();
    let version = tasty::package_table_version();
    let spec = Spec {
        sources: &sources,
        keep,
        extra: &vm::nir_table,
        derived: &[Derived { name: tasty::PACKAGE_TABLE, version: &version, contents: &packages }],
        tag: "nir member tables 1",
    };
    Ok(Box::leak(Box::new(Image::open(&spec, &cache_dir())?)))
}

/// The Scala standard library's jar, with its TASTy, if it is there.
fn scala_library() -> Option<PathBuf> {
    let file = format!("scala-library-{SCALA}.jar");
    let rel = format!("https/repo1.maven.org/maven2/org/scala-lang/scala-library/{SCALA}/{file}");
    let libs = std::env::var_os("SCALAI_LIBS").map(|l| PathBuf::from(l).join(&file));
    libs.into_iter().chain(coursier_roots().into_iter().map(|root| root.join(&rel))).find(|p| p.exists())
}

/// The roots of the Coursier caches to search.
fn coursier_roots() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    std::env::var_os("COURSIER_CACHE")
        .map(PathBuf::from)
        .into_iter()
        .chain([home.join("Library/Caches/Coursier/v1"), home.join(".cache/coursier/v1")])
        .collect()
}

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
    let rel = format!("https/repo1.maven.org/maven2/org/scala-native/{dir}/{version}/{file}");
    out.extend(coursier_roots().into_iter().map(|root| (root, rel.clone())));
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

/// Where the library image lives: `$SCALAI_CACHE`, else the user's cache directory.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SCALAI_CACHE") {
        return PathBuf::from(dir);
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("scalai"),
        None if cfg!(target_os = "macos") => home.join("Library/Caches/scalai"),
        None => home.join(".cache/scalai"),
    }
}
