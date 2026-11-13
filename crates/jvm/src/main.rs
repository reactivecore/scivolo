use clap::Parser;
use jvmgen::ClassBytes;
use lower::{Isolated, Options};
use macros::{Macros, VmMacros};
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;

/// Compiles Scala files through the Rust frontend into JVM class files and TASTy files; the
/// files are typed as one unit, with the options of their `//> using options` directives. When the
/// frontend refuses a file, the refused files are reported, nothing is written and the run fails.
///
/// Environment: SCIVOLO_LIBS holds the library jars (default: the distribution's, else the
/// Coursier cache).
#[derive(Parser)]
#[command(name = "scivolo-jvm")]
struct Cli {
    /// A jar or directory of TASTy and Java class files the sources see.
    #[arg(long)]
    cp: Vec<PathBuf>,
    /// A jar or directory of NIR files compiled before; macro calls run in a VM over the library's NIR
    /// and these.
    #[arg(long)]
    macro_cp: Vec<PathBuf>,
    /// The directory to write the class files to, or a jar if it ends in `.jar`.
    #[arg(short)]
    d: PathBuf,
    /// The jar manifest's `Main-Class` (dotty's `-Xmain-class`).
    #[arg(long)]
    main_class: Option<String>,
    /// The sources are Scala's library's: its classes are compiled in the same run.
    #[arg(long)]
    scalalib: bool,
    /// A compiler option of all sources, checked as the options of their directives.
    #[arg(long = "scalac-option", allow_hyphen_values = true)]
    scalac_options: Vec<String>,
    /// Writes the classes of the files the frontend accepts, leaving out the refused ones and the
    /// files that need them, then fails; for measurements only.
    #[arg(long)]
    leave_out_refused: bool,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
}

fn main() {
    let Cli {
        cp,
        macro_cp,
        d: out,
        main_class,
        scalalib,
        scalac_options: mut options,
        leave_out_refused,
        files,
    } = Cli::parse();
    let library = library_jars().unwrap_or_else(|e| fail(&e));
    let jars = [&library[..], &cp].concat();
    let on_the_jvm: Vec<usize> = std::iter::once(0).chain(library.len()..jars.len()).collect();
    let mut open = || {
        let mut u = Universe::open(&jars).unwrap_or_else(|e| fail(&format!("opening the library: {e:?}")));
        u.set_jdk(classfile::Jdk::default());
        u.use_java_classfiles(&on_the_jvm)
            .unwrap_or_else(|e| fail(&format!("reading the class files of {jars:?}: {e}")));
        if scalalib {
            u.set_same_run_jar(0);
        }
        u
    };
    let sources: Vec<SourceFile> = files
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(f).unwrap_or_else(|e| fail(&format!("reading {f}: {e}")));
            let directives = lower::directives::directives(&text, "3.9.0");
            let directives = directives.unwrap_or_else(|d| fail(&format!("the typer does not support {d}")));
            options.extend(directives.options.into_iter().map(str::to_owned));
            SourceFile::new(relative_path(f), text)
        })
        .collect();
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let Options { config, macros: settings, .. } = Options::checked(&options)
        .unwrap_or_else(|o| fail(&format!("the typer does not support option `{o}`")));
    let mut vm = (!macro_cp.is_empty()).then(|| {
        let nir = libs::jars().unwrap_or_else(|e| fail(&e));
        let classpath =
            vm::Classpath::with_jars(&macro_cp, &nir).unwrap_or_else(|e| fail(&format!("{macro_cp:?}: {e}")));
        vm::Vm::new(classpath).unwrap_or_else(|e| fail(&format!("the macro VM: {e}")))
    });
    let copies = sources.iter().map(|s| SourceFile::new(s.name.clone(), s.text.clone())).collect();
    let mut runner = vm.as_mut().map(|vm| VmMacros { vm, sources: copies, settings });
    let macros = runner.as_mut().map(|r| r as &mut dyn Macros);
    let backend = (lower::Backend::Jvm, jvmgen::unit);
    let message = |e: &lower::Refusal| e.what.replace("\n  ", " ");
    let Isolated { out: classes, refused, .. } =
        lower::compile_isolated(&mut open, &sources, config, macros, backend)
            .unwrap_or_else(|e| fail(&message(&e)));
    if refused.is_empty() || leave_out_refused {
        write(&out, &entries(&classes), main_class.as_deref());
    }
    for r in &refused {
        eprintln!("{}: {}", r.file, message(&r.refusal));
    }
    if !refused.is_empty() {
        std::process::exit(1);
    }
}

/// The jars the sources are typed against, Scala's standard library first: those
/// `typer::dump::library_jars` lists, found as the VM finds its libraries.
fn library_jars() -> Result<Vec<PathBuf>, String> {
    let mut out = vec![libs::scala_library()?];
    for lib in ["nativelib", "clib", "posixlib"] {
        out.push(libs::jar(lib)?);
    }
    out.extend([libs::JAVALIB_TASTY_JAR, libs::JDK_DECLARATIONS_JAR].map(libs::checked_in));
    Ok(out)
}

/// Writes `entries` into the jar or directory `out`, the jar's manifest naming `main_class`.
fn write(out: &Path, entries: &[(String, &[u8])], main_class: Option<&str>) {
    if out.extension().is_some_and(|e| e == "jar") {
        jvmgen::jar::write(out, entries, main_class)
            .unwrap_or_else(|e| fail(&format!("writing {out:?}: {e}")));
        return;
    }
    for &(ref name, bytes) in entries {
        let path = out.join(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| fail(&format!("creating {dir:?}: {e}")));
        }
        std::fs::write(&path, bytes).unwrap_or_else(|e| fail(&format!("writing {path:?}: {e}")));
    }
}

/// The class and TASTy files of `classes`, each its path from the output root and its bytes.
fn entries(classes: &[ClassBytes]) -> Vec<(String, &[u8])> {
    let class_files = classes.iter().map(|c| (format!("{}.class", c.name), &c.bytes[..]));
    let tasty = classes.iter().filter_map(|c| Some((format!("{}.tasty", c.name), &c.tasty.as_ref()?[..])));
    class_files.chain(tasty).collect()
}

/// Path `f` from the working directory, the source root (dotty's `SourceFile.relativePath`).
fn relative_path(f: &str) -> String {
    let cwd = std::env::current_dir().ok();
    let under = cwd.as_deref().and_then(|cwd| Path::new(f).strip_prefix(cwd).ok());
    under.map_or_else(|| f.to_owned(), |p| p.to_string_lossy().into_owned())
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}
