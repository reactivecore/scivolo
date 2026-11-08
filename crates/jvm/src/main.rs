use clap::Parser;
use jvmgen::ClassBytes;
use lower::Options;
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;

/// Compiles Scala files through the Rust frontend into JVM class files and TASTy files; the
/// files are typed as one unit, with the options of their `//> using options` directives.
///
/// Environment: SCIVOLO_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "scivolo-jvm")]
struct Cli {
    /// A jar or directory of TASTy files the sources see.
    #[arg(long)]
    cp: Vec<PathBuf>,
    /// The directory to write the class files to, or a jar if it ends in `.jar`.
    #[arg(short)]
    d: PathBuf,
    /// The jar manifest's `Main-Class` (dotty's `-Xmain-class`); by default the one class with
    /// a `main` method, if there is one.
    #[arg(long)]
    main_class: Option<String>,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
}

fn main() {
    let Cli { cp, d: out, main_class, files } = Cli::parse();
    let libs = std::env::var_os("SCIVOLO_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let jars = [&typer::dump::library_jars(&libs)[..], &cp].concat();
    let mut u = Universe::open(&jars).unwrap_or_else(|e| fail(&format!("opening the library: {e:?}")));
    let mut options = Vec::new();
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
    let Options { config, .. } = Options::checked(&options)
        .unwrap_or_else(|o| fail(&format!("the typer does not support option `{o}`")));
    let generate = (lower::Backend::Jvm, jvmgen::unit);
    let classes = match lower::compile_units(&mut u, &sources, config, None, generate) {
        Ok((classes, _)) => classes,
        Err(e) => fail(&e.what.replace("\n  ", " ")),
    };
    let entries = entries(&classes);
    if out.extension().is_some_and(|e| e == "jar") {
        let mains: Vec<&ClassBytes> = classes.iter().filter(|c| c.main).collect();
        let found = match mains[..] {
            [c] => Some(c.name.replace('/', ".")),
            _ => None,
        };
        let main_class = main_class.or(found);
        jvmgen::jar::write(&out, &entries, main_class.as_deref())
            .unwrap_or_else(|e| fail(&format!("writing {out:?}: {e}")));
        return;
    }
    for (name, bytes) in entries {
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
