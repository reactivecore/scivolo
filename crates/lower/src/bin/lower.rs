use clap::Parser;
use std::path::PathBuf;
use syntax::source::SourceFile;
use tasty::Universe;

/// Lowers a Scala file through the Rust frontend into NIR files.
///
/// Environment: SCALAI_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "lower")]
struct Cli {
    /// The Scala file.
    file: String,
    /// The directory to write the NIR files to, emptied first.
    out_dir: PathBuf,
}

fn main() {
    let Cli { file, out_dir: out } = Cli::parse();
    let libs = std::env::var_os("SCALAI_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let mut u = Universe::open(&typer::dump::library_jars(&libs)).expect("opening the library");
    let text = std::fs::read_to_string(&file).expect("reading the source");
    let name =
        std::path::Path::new(&file).file_name().map_or(file.clone(), |n| n.to_string_lossy().into_owned());
    match lower::compile(&mut u, &SourceFile::new(name, text), &[]) {
        Ok(files) => {
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir_all(&out).expect("creating the output directory");
            for f in files {
                std::fs::write(out.join(format!("{}.nir", f.name)), f.bytes).expect("writing NIR");
                println!("{}.nir", f.name);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
