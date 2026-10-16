//! `scalai-aot`: compiles Scala sources or NIR into a native executable with Cranelift.

mod blob;
mod classes;
mod codegen;
mod entry;
mod frontend;
mod link;
mod reach;
mod sources;
mod tables;

use clap::Parser;
use std::path::PathBuf;

pub type R<T> = Result<T, String>;

/// Compile Scala sources, or the NIR of a classpath, into a native executable.
#[derive(Parser)]
#[command(name = "scalai-aot")]
struct Args {
    /// Directory of `.nir` files to link, or jar whose TASTy and NIR the sources use.
    #[arg(long = "cp")]
    classpath: Vec<PathBuf>,
    /// Name of an `(Int) => Int` method the executable calls with its argument, instead of a
    /// main class.
    #[arg(long, conflicts_with = "main")]
    entry: Option<String>,
    /// Path of the executable.
    #[arg(short = 'o', required_unless_present = "out_dir")]
    output: Option<PathBuf>,
    /// Directory the NIR of the sources goes to, instead of an executable.
    #[arg(short = 'd', conflicts_with = "output")]
    out_dir: Option<PathBuf>,
    /// The object or class whose `main` the executable runs; needed when several have one.
    #[arg(long)]
    main: Option<String>,
    /// `.scala` files and directories of them; without any, a main class of the classpath.
    inputs: Vec<String>,
}

fn run(args: &Args) -> R<()> {
    let (jars, dirs): (Vec<PathBuf>, Vec<PathBuf>) =
        args.classpath.iter().cloned().partition(|p| p.extension().is_some_and(|e| e == "jar"));
    let mut lib_jars = libs::jars()?;
    lib_jars.extend(jars);
    let image = libs::image(&lib_jars).map_err(|e| format!("library image: {e}"))?;
    let (srcs, named): (Vec<String>, Vec<String>) =
        args.inputs.iter().cloned().partition(|a| sources::is_source(a));
    if let Some(path) = named.iter().find(|n| n.contains('/')) {
        return Err(format!("{path}: no such directory"));
    }
    if args.out_dir.is_some() && (srcs.is_empty() || args.main.is_some() || args.entry.is_some()) {
        return Err("-d writes the NIR of sources; it takes sources and neither --main nor --entry".into());
    }
    let mut classpath = dirs;
    let main = match (srcs.is_empty(), &named[..], &args.main) {
        (true, [_], Some(_)) => return Err("a main class both as an argument and with --main".into()),
        (true, [main], None) | (true, [], Some(main)) => Some(main.clone()),
        (true, [], None) => None,
        (true, _, _) => return Err(format!("several main classes given: {}", named.join(" "))),
        (false, [], _) => {
            let unit = frontend::read(&sources::collect(&srcs)?)?;
            if let Some(out) = &args.out_dir {
                return frontend::compile_into(&unit, image, out);
            }
            let (nir, mains) = frontend::compile(&unit, &lib_jars, image)?;
            classpath.insert(0, nir);
            match &args.entry {
                Some(_) => None,
                None => Some(sources::pick(&mains, args.main.as_deref())?),
            }
        }
        (false, _, _) => {
            return Err(format!("sources and a main class name mixed ({}); use --main", named.join(" ")));
        }
    };
    let output = args.output.as_ref().ok_or("-o is required to link an executable")?;
    let classpath_entries = classpath.clone();
    let classpath = vm::Classpath::with_image(&classpath, image).map_err(|e| e.to_string())?;
    let mut world = vm::World::new(classpath);
    let (root, stub) = match (&args.entry, main) {
        (Some(entry), _) => {
            (codegen::Root::Entry(entry::find(&classpath_entries, entry, &mut world)?), "entry.c")
        }
        (None, Some(main)) => (codegen::Root::Main(world.top(&main)), "main.c"),
        (None, None) => return Err("a main class or --entry is required".into()),
    };
    let (object, shims) = codegen::compile(&mut world, root)?;
    link::executable(&object, &shims, stub, output)
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("scalai-aot: {e}");
        std::process::exit(1);
    }
}
