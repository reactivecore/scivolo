use libimage::{Image, Source, Spec};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tasty::{Kind, Universe};

fn jars() -> Option<Vec<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fetched = [
        "scala-library-3.9.0.jar",
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ]
    .map(|j| root.join("ref/libs").join(j));
    if let Some(missing) = fetched.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    let mut jars = fetched.to_vec();
    jars.push(root.join("libs/javalib-tasty-0.5.12.jar"));
    Some(jars)
}

/// The image of the TASTy of `jars`, in the repository's `tmp/`.
fn image(jars: &[PathBuf]) -> &'static Image {
    let sources: Vec<Source> = jars.iter().cloned().map(Source::File).collect();
    let spec = Spec {
        sources: &sources,
        keep: |n| n.ends_with(".tasty"),
        extra: &|_, _| Vec::new(),
        tag: "tasty test",
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/tasty-image");
    Box::leak(Box::new(Image::open(&spec, &dir).unwrap()))
}

fn class_names(u: &mut Universe, pkg: &str) -> Vec<String> {
    let p = u.package(pkg).unwrap();
    let mut names: Vec<String> = u
        .decls(p)
        .unwrap()
        .into_iter()
        .filter(|&d| u.symbol(d).kind == Kind::Class)
        .map(|d| u.full_name(d))
        .collect();
    names.sort();
    names
}

#[test]
fn an_image_holds_what_the_jars_hold() {
    let Some(jars) = jars() else { return };
    let mut from_jars = Universe::open(&jars).unwrap();
    let mut from_image = Universe::open_image(image(&jars));
    for pkg in ["scala.collection.immutable", "java.lang", "java.util"] {
        assert_eq!(class_names(&mut from_jars, pkg), class_names(&mut from_image, pkg), "{pkg}");
    }
}

#[test]
#[ignore = "measurement"]
fn open_times() {
    let Some(jars) = jars() else { return };
    let image = image(&jars);
    for _ in 0..3 {
        let start = Instant::now();
        let _ = Universe::open(&jars).unwrap();
        let from_jars = start.elapsed();
        let start = Instant::now();
        let _ = Universe::open_image(image);
        eprintln!("jars {from_jars:?}, image {:?}", start.elapsed());
    }
}
