//! Locates the precompiled Scala Native libraries and the Scala standard library's TASTy: in
//! `$SCIVOLO_LIBS` if set, then in the distribution, then in the Coursier cache; and maps their
//! library image.

pub mod deps;

use libimage::{Derived, Image, Source, Spec};
use std::io;
use std::path::{Path, PathBuf};

pub const SCALA: &str = "3.9.0";
pub const NATIVE: &str = "0.5.12";

/// A hash of the workspace's sources, which keys what the build derives from them.
pub const BUILD: &str = env!("SCIVOLO_BUILD");

const LIBS: [&str; 6] = ["auxlib", "javalib", "nativelib", "clib", "posixlib", "scalalib"];

/// The file name of the TASTy of Scala Native's javalib (research 08 §3).
pub const JAVALIB_TASTY_JAR: &str = "javalib-tasty-0.5.12.jar";

/// The file name of the TASTy of the JDK's declarations javalib's classes are typed from
/// (`just jdk-stubs`).
pub const JDK_DECLARATIONS_JAR: &str = "jdk-declarations.jar";

/// The jar named by `JAVALIB_TASTY_JAR`.
const JAVALIB_TASTY: &[u8] = include_bytes!("../../../libs/javalib-tasty-0.5.12.jar");

/// The jar named by `JDK_DECLARATIONS_JAR`.
const JDK_DECLARATIONS: &[u8] = include_bytes!("../../../libs/jdk-declarations.jar");

/// The library image of `jars`, the standard library's TASTy, javalib's and the JDK's
/// declarations: their NIR with member tables, and their TASTy with symbol tables and its
/// package table.
pub fn image(jars: &[PathBuf]) -> io::Result<&'static Image> {
    let mut sources: Vec<Source> = jars.iter().cloned().map(Source::File).collect();
    sources.extend(scala_library().ok().map(Source::File));
    sources.push(Source::Bytes(JAVALIB_TASTY_JAR, JAVALIB_TASTY));
    sources.push(Source::Bytes(JDK_DECLARATIONS_JAR, JDK_DECLARATIONS));
    let keep = |name: &str| name.ends_with(".nir") || name.ends_with(".tasty");
    let extra = |name: &str, data: &[u8]| match name.ends_with(".tasty") {
        true => tasty::symbol_table(data),
        false => vm::nir_table(name, data),
    };
    let tag =
        format!("nir member tables 1, tasty symbol tables {}, build {}", tasty::SYMBOL_TABLE_VERSION, BUILD);
    let packages = |names: &[&str]| tasty::package_table(names).into_bytes();
    let version = tasty::package_table_version();
    let spec = Spec {
        sources: &sources,
        keep,
        extra: &extra,
        derived: &[Derived { name: tasty::PACKAGE_TABLE, version: &version, contents: &packages }],
        tag: &tag,
    };
    Ok(Box::leak(Box::new(Image::open(&spec, &cache_dir())?)))
}

/// Locates the Scala standard library's jar, with its TASTy: in `$SCIVOLO_LIBS`, the
/// distribution's `lib/scala/` or the Coursier cache; or says where it is missing.
pub fn scala_library() -> Result<PathBuf, String> {
    let file = format!("scala-library-{SCALA}.jar");
    let rel = format!("https/repo1.maven.org/maven2/org/scala-lang/scala-library/{SCALA}/{file}");
    let flat = flat_dirs().into_iter().map(|dir| dir.join(&file));
    flat.chain(coursier_roots().into_iter().map(|root| root.join(&rel)))
        .find(|p| p.exists())
        .ok_or_else(|| missing("scala-library", SCALA))
}

/// The error of a library jar found nowhere.
fn missing(lib: &str, version: &str) -> String {
    format!("{lib} {version} not in $SCIVOLO_LIBS, the distribution or the Coursier cache")
}

/// The directories that hold the library jars by their file names: `$SCIVOLO_LIBS`, then the
/// distribution's `lib/scala/`.
fn flat_dirs() -> Vec<PathBuf> {
    let libs = std::env::var_os("SCIVOLO_LIBS").map(PathBuf::from);
    libs.into_iter().chain(dist::lib_dir().map(|lib| lib.join("scala"))).collect()
}

/// Locates checked-in jar `name` of `libs/`: in the distribution's `lib/`, else in the
/// workspace.
pub fn checked_in(name: &str) -> PathBuf {
    dist::file(name, Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libs").join(name))
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
    let mut out: Vec<(PathBuf, String)> = flat_dirs().into_iter().map(|dir| (dir, file.clone())).collect();
    let rel = format!("https/repo1.maven.org/maven2/org/scala-native/{dir}/{version}/{file}");
    out.extend(coursier_roots().into_iter().map(|root| (root, rel.clone())));
    out
}

/// The library jars, in classpath order, or the first one that is missing.
pub fn jars() -> Result<Vec<PathBuf>, String> {
    LIBS.iter().map(|lib| jar(lib)).collect()
}

/// Locates the jar of Scala Native library `lib` (e.g. `nativelib`), or says where it is missing.
pub fn jar(lib: &str) -> Result<PathBuf, String> {
    candidates(lib)
        .into_iter()
        .map(|(root, rel)| root.join(rel))
        .find(|p| p.exists())
        .ok_or_else(|| missing(lib, NATIVE))
}

/// Where the library image lives: `$SCIVOLO_CACHE`, else the user's cache directory.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SCIVOLO_CACHE") {
        return PathBuf::from(dir);
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("scivolo"),
        None if cfg!(target_os = "macos") => home.join("Library/Caches/scivolo"),
        None => home.join(".cache/scivolo"),
    }
}
