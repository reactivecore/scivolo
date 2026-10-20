//! `scivolo-aot`: compiles Scala sources or NIR into a native executable with Cranelift.

mod blob;
mod classes;
mod codegen;
mod entry;
mod frontend;
mod link;
mod reach;
mod resources;
mod sources;
mod tables;
mod timings;

use clap::Parser;
use std::path::PathBuf;

pub type R<T> = Result<T, String>;

/// Compile Scala sources, or the NIR of a classpath, into a native executable.
#[derive(Parser)]
#[command(name = "scivolo-aot")]
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
    /// A Scala Native library from Maven, with what it depends on, fetched by Coursier:
    /// `group::artifact::version` or `group::artifact:version` (the `_native0.5_3` artifact),
    /// or `group:artifact:version` as named; repeatable.
    #[arg(long = "dep", value_name = "COORDINATE")]
    deps: Vec<String>,
    /// Print the wall time of each phase to stderr.
    #[arg(long)]
    timings: bool,
    /// `.scala` files and directories of them; without any, a main class of the classpath.
    inputs: Vec<String>,
}

/// Fails on inputs and options that do not go together: sources, main class names (`named`),
/// `--main`, `--entry` and `-d`.
fn check(args: &Args, srcs: &[String], named: &[String]) -> R<()> {
    if let Some(path) = named.iter().find(|n| n.contains('/')) {
        return Err(format!("{path}: no such directory"));
    }
    if args.out_dir.is_some() && (srcs.is_empty() || args.main.is_some() || args.entry.is_some()) {
        return Err("-d writes the NIR of sources; it takes sources and neither --main nor --entry".into());
    }
    match (srcs.is_empty(), named, &args.main, &args.entry) {
        (true, [_], Some(_), _) => Err("a main class both as an argument and with --main".into()),
        (true, [], None, None) => Err("a main class or --entry is required".into()),
        (true, [_, _, ..], _, _) => Err(format!("several main classes given: {}", named.join(" "))),
        (false, [_, ..], _, _) => {
            Err(format!("sources and a main class name mixed ({}); use --main", named.join(" ")))
        }
        _ => Ok(()),
    }
}

fn run(args: &Args) -> R<()> {
    let mut timings = timings::Timings::new();
    let (srcs, named): (Vec<String>, Vec<String>) =
        args.inputs.iter().cloned().partition(|a| sources::is_source(a));
    check(args, &srcs, &named)?;
    let unit = match srcs.is_empty() {
        true => None,
        false => Some(frontend::read(&sources::collect(&srcs)?)?),
    };
    let (jars, dirs): (Vec<PathBuf>, Vec<PathBuf>) =
        args.classpath.iter().cloned().partition(|p| p.extension().is_some_and(|e| e == "jar"));
    let mut deps = args.deps.clone();
    deps.extend(unit.iter().flat_map(|u| u.deps().iter().cloned()));
    let mut lib_jars = libs::jars()?;
    lib_jars.extend(jars);
    // Temporary until #115 (native code of dependencies): the C sources in dependency jars are
    // not compiled into the executable.
    lib_jars.extend(libs::deps::fetch(&deps)?);
    let image = libs::image(&lib_jars).map_err(|e| format!("library image: {e}"))?;
    let mut classpath = dirs;
    let main = match (unit, &named[..], &args.main) {
        (None, [main], _) | (None, [], Some(main)) => Some(main.clone()),
        (None, _, _) => None,
        (Some(unit), _, _) => {
            if let Some(out) = &args.out_dir {
                return frontend::compile_into(&unit, image, out);
            }
            let (nir, mains) = frontend::compile(&unit, &lib_jars, image)?;
            timings.lap("frontend");
            classpath.insert(0, nir);
            match &args.entry {
                Some(_) => None,
                None => Some(sources::pick(&mains, args.main.as_deref())?),
            }
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
    timings.lap("load");
    let (object, shims) = codegen::compile(&mut world, root, &mut timings)?;
    link::executable(&object, &shims, stub, output)?;
    timings.lap("link");
    if args.timings {
        timings.print();
    }
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("scivolo-aot: {e}");
        std::process::exit(1);
    }
}
