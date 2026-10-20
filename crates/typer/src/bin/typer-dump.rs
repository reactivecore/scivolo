use clap::Parser;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump, library_jars};

/// Prints what the typer makes of Scala files, in the format of `scripts/typer-oracle`.
///
/// Environment: SCIVOLO_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "typer-dump")]
struct Cli {
    /// The files, one `path<TAB>options` per line.
    #[arg(long, value_name = "LIST")]
    list: PathBuf,
    /// Print type errors even when the unit also hits an unsupported construct.
    #[arg(long)]
    errors_first: bool,
    /// Expand inline calls and print only the outermost `Inlined` trees.
    #[arg(long)]
    inlined: bool,
}

fn main() {
    let Cli { list, errors_first, inlined } = Cli::parse();
    let libs = std::env::var_os("SCIVOLO_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let jars = library_jars(&libs);
    let jobs = std::fs::read_to_string(list).expect("reading the list");
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in jobs.lines().filter(|l| !l.is_empty()) {
        let (path, options) = line.split_once('\t').unwrap_or((line, ""));
        let options: Vec<&str> = options.split(' ').filter(|o| !o.is_empty()).collect();
        let mut b = format!("== file {path}\n");
        let mut jars = jars.clone();
        let classpath = options.windows(2).filter(|w| w[0] == "-classpath").flat_map(|w| w[1].split(':'));
        jars.extend(classpath.map(PathBuf::from));
        let run = std::panic::catch_unwind(|| {
            let mut u = Universe::open(&jars).expect("opening the library");
            let text = std::fs::read(path).expect("reading the source");
            dump(
                &mut u,
                &SourceFile::new(path, String::from_utf8_lossy(&text)),
                &options,
                errors_first,
                inlined,
            )
        });
        match run {
            Ok(text) => b.push_str(&text),
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()));
                writeln!(b, "== crash {}", msg.unwrap_or_default().replace('\n', " ")).unwrap();
            }
        }
        out.write_all(b.as_bytes()).unwrap();
    }
}
