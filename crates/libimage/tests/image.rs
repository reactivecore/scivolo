use libimage::{Image, Source, Spec};
use std::path::Path;

const JAR: &[u8] = include_bytes!("../../../libs/javalib-tasty-0.5.12.jar");

#[test]
fn a_truncated_image_is_rebuilt() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tmp/libimage-test")
        .join(std::process::id().to_string());
    let sources = [Source::Bytes("javalib-tasty.jar", JAR)];
    let spec =
        Spec { sources: &sources, keep: |n| n.ends_with(".tasty"), extra: &|_, _| Vec::new(), tag: "test" };
    let string = "java/lang/_String.tasty";
    let size = {
        let image = Image::open(&spec, &dir).unwrap();
        image.find(string).expect("String's TASTy").0.len()
    };
    let path = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "img"))
        .unwrap();
    let full = std::fs::read(&path).unwrap();
    std::fs::write(&path, &full[..full.len() / 2]).unwrap();
    let image = Image::open(&spec, &dir).unwrap();
    assert_eq!(image.find(string).expect("String's TASTy").0.len(), size);
    drop(image);
    let _ = std::fs::remove_dir_all(&dir);
}
