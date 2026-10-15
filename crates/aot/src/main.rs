//! `scalai-aot`: compiles NIR into a native executable with Cranelift.

mod classes;
mod codegen;
mod entry;
#[path = "../../scalai/src/libs.rs"]
mod libs;
mod link;
mod reach;
mod tables;

use clap::Parser;
use std::path::PathBuf;

pub type R<T> = Result<T, String>;

/// Compile NIR into a native executable.
#[derive(Parser)]
#[command(name = "scalai-aot")]
struct Args {
    /// Directory of `.nir` files.
    #[arg(long = "cp", required = true)]
    classpath: Vec<PathBuf>,
    /// Name of the `(Int) => Int` method the executable calls with its argument.
    // Temporary until #113 aot 3 (generated main): replaced by the main class.
    #[arg(long)]
    entry: String,
    /// Path of the executable.
    #[arg(short = 'o')]
    output: PathBuf,
}

fn run(args: &Args) -> R<()> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| format!("library image: {e}"))?;
    let classpath = vm::Classpath::with_image(&args.classpath, image).map_err(|e| e.to_string())?;
    let mut world = vm::World::new(classpath);
    let entry = entry::find(&args.classpath, &args.entry, &mut world)?;
    let object = codegen::compile(&mut world, entry)?;
    link::executable(&object, &args.output)
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("scalai-aot: {e}");
        std::process::exit(1);
    }
}
