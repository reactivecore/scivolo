// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/io/FileWriters.scala
//! A jar of class and TASTy files as dotty writes one for `-d out.jar` (`JarEntryWriter`).

use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

/// Writes `files`, each its path in the jar and its bytes, to jar `path` after a manifest that
/// names `main_class` if given.
pub fn write(path: &Path, files: &[(String, &[u8])], main_class: Option<&str>) -> zip::result::ZipResult<()> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("META-INF/MANIFEST.MF", options)?;
    zip.write_all(manifest(main_class).as_bytes())?;
    for (name, bytes) in files {
        zip.start_file(name.as_str(), options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(())
}

/// The manifest in `java.util.jar.Manifest`'s format: lines of at most 72 bytes, each longer
/// one continued on lines that start with a space.
fn manifest(main_class: Option<&str>) -> String {
    let mut attributes = vec![("Manifest-Version", "1.0"), ("Scala-Compiler-Version", "3.9.0")];
    attributes.extend(main_class.map(|c| ("Main-Class", c)));
    let mut out = String::new();
    for (key, value) in attributes {
        let line = format!("{key}: {value}");
        let mut rest = line.as_str();
        let mut limit = 72;
        while rest.len() > limit {
            let at = (0..=limit).rev().find(|&i| rest.is_char_boundary(i)).unwrap_or(0);
            out.push_str(&rest[..at]);
            out.push_str("\r\n ");
            rest = &rest[at..];
            limit = 71;
        }
        out.push_str(rest);
        out.push_str("\r\n");
    }
    out + "\r\n"
}

#[cfg(test)]
mod tests {
    #[test]
    fn long_lines_continue_after_72_bytes() {
        let class = "a".repeat(80);
        let m = super::manifest(Some(&class));
        let lines: Vec<&str> = m.split("\r\n").collect();
        assert_eq!(lines[2].len(), 72);
        assert_eq!(lines[3], format!(" {}", "a".repeat(80 + 12 - 72)));
    }
}
