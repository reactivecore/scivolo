//! `scalai-aot`: compiles NIR into a native executable with Cranelift.

mod blob;
mod classes;
mod codegen;
mod entry;
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
    /// Name of an `(Int) => Int` method the executable calls with its argument, instead of a
    /// main class.
    // Temporary until #113 aot 3 (generated main) runs every test program: then dropped.
    #[arg(long, conflicts_with = "main")]
    entry: Option<String>,
    /// Path of the executable.
    #[arg(short = 'o')]
    output: PathBuf,
    /// The object or class whose `main` the executable runs.
    #[arg(required_unless_present = "entry")]
    main: Option<String>,
}

fn run(args: &Args) -> R<()> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| format!("library image: {e}"))?;
    let classpath = vm::Classpath::with_image(&args.classpath, image).map_err(|e| e.to_string())?;
    let mut world = vm::World::new(classpath);
    let (root, stub) = match (&args.entry, &args.main) {
        (Some(entry), _) => {
            (codegen::Root::Entry(entry::find(&args.classpath, entry, &mut world)?), "entry.c")
        }
        (None, Some(main)) => (codegen::Root::Main(world.top(main)), "main.c"),
        (None, None) => return Err("a main class or --entry is required".into()),
    };
    let (object, shims) = codegen::compile(&mut world, root)?;
    link::executable(&object, &shims, stub, &args.output)
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("scalai-aot: {e}");
        std::process::exit(1);
    }
}
