//! `scalai-aot`: compiles NIR into a native executable with Cranelift.

mod codegen;
mod link;

use clap::Parser;
use std::path::{Path, PathBuf};

/// Compile NIR into a native executable.
#[derive(Parser)]
#[command(name = "scalai-aot")]
struct Args {
    /// Directory of `.nir` files.
    #[arg(long = "cp", required = true)]
    classpath: Vec<PathBuf>,
    /// Name of the `(Int) => Int` method the executable calls with its first argument.
    #[arg(long)]
    entry: String,
    /// Path of the executable.
    #[arg(short = 'o')]
    output: PathBuf,
}

fn nir_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "nir")).collect();
    files.sort();
    Ok(files)
}

fn load(dirs: &[PathBuf], pool: &mut nir::Pool) -> Result<Vec<nir::Defn>, String> {
    let mut caches = nir::Caches::default();
    let mut defns = Vec::new();
    for dir in dirs {
        for file in nir_files(dir)? {
            let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let mut reader = nir::Reader::new(&bytes, &mut caches, pool)
                .map_err(|e| format!("{}: {e}", file.display()))?;
            reader.debug_info = false;
            defns.extend(reader.all().map_err(|e| format!("{}: {e}", file.display()))?);
        }
    }
    Ok(defns)
}

fn run(args: &Args) -> Result<(), String> {
    let mut pool = nir::Pool::default();
    let defns = load(&args.classpath, &mut pool)?;
    let object = codegen::compile(&pool, &defns, &args.entry)?;
    link::executable(&object, &args.output)
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("scalai-aot: {e}");
        std::process::exit(1);
    }
}
