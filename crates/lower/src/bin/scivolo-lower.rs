use clap::Parser;
use lower::Options;
use macros::{Macros, VmMacros};
use std::path::{Path, PathBuf};
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
    /// Like `--tasty`, its classes standing for the JDK's: with `--javalib`, as Java classes;
    /// without, ahead of the library's, in place of javalib's classes of the same names.
    #[arg(long)]
    jdk_tasty: Vec<PathBuf>,
    /// The sources are javalib's: javalib's classes are Scala classes, not the JDK's.
    #[arg(long)]
    javalib: bool,
    /// The sources are Scala's library's: its classes are compiled in the same run.
    #[arg(long, conflicts_with = "javalib")]
    scalalib: bool,
    /// The sources see the JDK 17 API: no forwarders to javalib's methods JDK 17 lacks.
    #[arg(long, conflicts_with = "javalib")]
    jdk_17: bool,
    /// Macro calls run in a VM over the library's NIR, also without `--cp`.
    #[arg(long)]
    macros: bool,
    /// Names each source in positions by its path relative to this directory, not by its file name.
    #[arg(long)]
    sourceroot: Option<PathBuf>,
    /// Names each source in NIR positions by its path relative to this directory instead, as
    /// Scala Native's `positionRelativizationPaths`; TASTy keeps the path from `--sourceroot`.
    #[arg(long, requires = "sourceroot")]
    position_root: Option<PathBuf>,
    /// A compiler option of all sources, checked as the options of their directives.
    #[arg(long = "scalac-option", allow_hyphen_values = true)]
    scalac_options: Vec<String>,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
    /// The directory to write the NIR files to, emptied first.
    out_dir: PathBuf,
}

fn main() {
    let Cli {
        cp,
        tasty,
        jdk_tasty,
        javalib,
        scalalib,
        jdk_17,
        macros,
        sourceroot,
        position_root,
        scalac_options,
        files,
        out_dir: out,
    } = Cli::parse();
    let libs = std::env::var_os("SCIVOLO_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let jars = [&typer::dump::library_jars(&libs)[..], &cp, &tasty].concat();
    let (all, jdk) = match javalib {
        true => ([&jars[..], &jdk_tasty].concat(), jars.len()..jars.len() + jdk_tasty.len()),
        false => ([&jdk_tasty[..], &jars].concat(), 0..jdk_tasty.len()),
    };
    let mut u = Universe::open(&all).expect("opening the library");
    let scala_library = jdk.end;
    u.set_jdk_jars(jdk);
    if javalib {
        u.set_javalib_sources();
    }
    if scalalib {
        u.set_same_run_jar(scala_library);
    }
    if jdk_17 {
        u.set_jdk_17_view();
    }
    let sources: Vec<SourceFile> = files
        .iter()
        .map(|f| {
            let path = source_name(f, sourceroot.as_deref());
            let Some(root) = position_root.as_deref() else { return SourceFile::new(path, read(f)) };
            let mut file = SourceFile::new(source_name(f, Some(root)), read(f));
            file.tasty_path = Some(path);
            file
        })
        .collect();
    let mut vm = (macros || !cp.is_empty()).then(|| {
        let nir = libs::jars().unwrap_or_else(|e| fail(&e));
        let classpath = vm::Classpath::with_jars(&cp, &nir).unwrap_or_else(|e| fail(&format!("{cp:?}: {e}")));
        vm::Vm::new(classpath).unwrap_or_else(|e| fail(&format!("the macro VM: {e}")))
    });
    let mut options: Vec<&str> = scalac_options.iter().map(String::as_str).collect();
    for s in &sources {
        let directives = lower::directives::directives(&s.text, "3.9.0");
        options
            .extend(directives.unwrap_or_else(|d| fail(&format!("the typer does not support {d}"))).options);
    }
    let Options { config: cfg, fatal, macros: settings } = Options::checked(&options)
        .unwrap_or_else(|o| fail(&format!("the typer does not support option `{o}`")));
    let copies = || sources.iter().map(|s| SourceFile::new(s.name.clone(), s.text.clone())).collect();
    let mut runner = vm.as_mut().map(|vm| VmMacros { vm, sources: copies(), settings });
    let macros = runner.as_mut().map(|r| r as &mut dyn Macros);
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
