use clap::Parser;
use lower::Options;
use std::path::PathBuf;
use syntax::source::SourceFile;
use tasty::Universe;

/// Compiles Scala files through the Rust frontend into JVM class files; the files are typed as
/// one unit, with the options of their `//> using options` directives.
///
/// Environment: SCIVOLO_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "scivolo-jvm")]
struct Cli {
    /// A jar or directory of TASTy files the sources see.
    #[arg(long)]
    cp: Vec<PathBuf>,
    /// The directory to write the class files to.
    #[arg(short)]
    d: PathBuf,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
}

fn main() {
    let Cli { cp, d: out, files } = Cli::parse();
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
            let name =
                std::path::Path::new(f).file_name().map_or(f.clone(), |n| n.to_string_lossy().into_owned());
            SourceFile::new(name, text)
        })
        .collect();
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let Options { config, .. } = Options::checked(&options)
        .unwrap_or_else(|o| fail(&format!("the typer does not support option `{o}`")));
    let classes = match lower::compile_units(&mut u, &sources, config, None, jvmgen::unit) {
        Ok((classes, _)) => classes,
        Err(e) => fail(&e.what.replace("\n  ", " ")),
    };
    for c in classes {
        let path = out.join(format!("{}.class", c.name));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| fail(&format!("creating {dir:?}: {e}")));
        }
        std::fs::write(&path, c.bytes).unwrap_or_else(|e| fail(&format!("writing {path:?}: {e}")));
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}
