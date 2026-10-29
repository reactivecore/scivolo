use clap::Parser;
use macros::{Macros, Settings, VmMacros};
use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

/// Lowers Scala files through the Rust frontend into NIR files; several files are typed as one
/// unit, with the options of their `//> using options` directives; other directives and options
/// the frontend does not implement are refused. With `--cp`, they see the TASTy of those
/// directories, and macro calls run in a VM over the library and their NIR.
///
/// Environment: SCIVOLO_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "scivolo-lower")]
struct Cli {
    /// A directory of TASTy and NIR files compiled before.
    #[arg(long)]
    cp: Vec<PathBuf>,
    /// A jar or directory whose TASTy the sources see, without its NIR.
    #[arg(long)]
    tasty: Vec<PathBuf>,
    /// The sources are javalib's: javalib's classes are Scala classes, not the JDK's.
    #[arg(long)]
    javalib: bool,
    /// Names each source in positions by its path relative to this directory, not by its file name.
    #[arg(long)]
    sourceroot: Option<PathBuf>,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
    /// The directory to write the NIR files to, emptied first.
    out_dir: PathBuf,
}

fn main() {
    let Cli { cp, tasty, javalib, sourceroot, files, out_dir: out } = Cli::parse();
    let libs = std::env::var_os("SCIVOLO_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let jars = typer::dump::library_jars(&libs);
    let mut u = Universe::open(&[&jars[..], &cp, &tasty].concat()).expect("opening the library");
    if javalib {
        u.set_javalib_sources();
    }
    let sources: Vec<SourceFile> =
        files.iter().map(|f| SourceFile::new(source_name(f, sourceroot.as_deref()), read(f))).collect();
    let mut vm = (!cp.is_empty()).then(|| {
        let nir = libs::jars().unwrap_or_else(|e| fail(&e));
        let classpath = vm::Classpath::with_jars(&cp, &nir).unwrap_or_else(|e| fail(&format!("{cp:?}: {e}")));
        vm::Vm::new(classpath).unwrap_or_else(|e| fail(&format!("the macro VM: {e}")))
    });
    let mut options: Vec<&str> = Vec::new();
    for s in &sources {
        let directives = lower::directives::directives(&s.text, "3.9.0");
        options
            .extend(directives.unwrap_or_else(|d| fail(&format!("the typer does not support {d}"))).options);
    }
    let macro_settings = options.iter().filter_map(|o| o.strip_prefix("-Xmacro-settings:"));
    let settings = Settings {
        retain_trees: options.contains(&"-Yretain-trees"),
        macro_settings: macro_settings.flat_map(|s| s.split(',')).map(str::to_owned).collect(),
    };
    options.retain(|o| !o.starts_with("-Xmacro-settings:"));
    let copies = || sources.iter().map(|s| SourceFile::new(s.name.clone(), s.text.clone())).collect();
    let mut runner = vm.as_mut().map(|vm| VmMacros { vm, sources: copies(), settings });
    let macros = runner.as_mut().map(|r| r as &mut dyn Macros);
    let fatal =
        options.iter().any(|o| ["-Werror", "-Xfatal-warnings"].contains(o) || o.starts_with("-Wconf"));
    options.retain(|o| !no_code_effect(o) || Config::checked(&[o]).is_ok());
    let cfg = Config::checked(&options)
        .unwrap_or_else(|o| fail(&format!("the typer does not support option `{o}`")));
    let lowered = match &sources[..] {
        [one] => lower::compile_with(&mut u, one, cfg, macros),
        many => lower::compile_sources(&mut u, many, cfg, macros),
    };
    match lowered {
        Ok((_, warnings)) if fatal && !warnings.items.is_empty() => {
            fail("the typer does not support warnings under -Werror or -Wconf")
        }
        Ok((files, _)) => {
            let _ = std::fs::remove_dir_all(&out);
            std::fs::create_dir_all(&out).expect("creating the output directory");
            for f in files {
                let path = out.join(format!("{}.nir", f.name.replace('.', "/")));
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).expect("creating a package directory");
                }
                std::fs::write(&path, f.bytes).expect("writing NIR");
                println!("{}.nir", f.name);
            }
        }
        Err(e) => {
            let error = e.diags.items.iter().find(|d| d.level == syntax::diag::Level::Error);
            fail(&match error {
                Some(d) => format!("{}: {}", e.what, d.message.replace('\n', " ")),
                None => e.what.replace("\n  ", " "),
            })
        }
    }
}

/// Whether option `o` cannot change the code of a program dotty compiles.
fn no_code_effect(o: &str) -> bool {
    matches!(
        o,
        "-Werror"
            | "-Xfatal-warnings"
            | "-experimental"
            | "-deprecation"
            | "-feature"
            | "-unchecked"
            | "-Yretain-trees"
            | "-Xcheck-macros"
    ) || ["-W", "-opt", "-Ycheck"].iter().any(|p| o.starts_with(p))
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}

fn source_name(file: &str, root: Option<&Path>) -> String {
    let path = Path::new(file);
    let name = match root {
        Some(root) => {
            path.strip_prefix(root).unwrap_or_else(|_| fail(&format!("{file} is not under {root:?}")))
        }
        None => path.file_name().map_or(path, Path::new),
    };
    name.to_string_lossy().into_owned()
}

fn read(file: &str) -> String {
    std::fs::read_to_string(file).unwrap_or_else(|e| panic!("reading {file}: {e}"))
}
