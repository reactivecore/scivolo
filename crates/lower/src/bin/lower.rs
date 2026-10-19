use clap::Parser;
use macros::{Macros, VmMacros};
use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

/// Lowers Scala files through the Rust frontend into NIR files; several files are typed as one
/// unit. With `--cp`, they see the TASTy of those directories, and macro calls run in a VM over
/// the library and their NIR.
///
/// Environment: SCALAI_LIBS holds the library jars (default: ref/libs).
#[derive(Parser)]
#[command(name = "lower")]
struct Cli {
    /// A directory of TASTy and NIR files compiled before.
    #[arg(long)]
    cp: Vec<PathBuf>,
    /// The Scala files.
    #[arg(required = true)]
    files: Vec<String>,
    /// The directory to write the NIR files to, emptied first.
    out_dir: PathBuf,
}

fn main() {
    let Cli { cp, files, out_dir: out } = Cli::parse();
    let libs = std::env::var_os("SCALAI_LIBS")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"), PathBuf::from);
    let jars = typer::dump::library_jars(&libs);
    let mut u = Universe::open(&[&jars[..], &cp].concat()).expect("opening the library");
    let sources: Vec<SourceFile> = files.iter().map(|f| SourceFile::new(file_name(f), read(f))).collect();
    let mut vm = (!cp.is_empty()).then(|| {
        let nir = libs::jars().unwrap_or_else(|e| fail(&e));
        let classpath = vm::Classpath::with_jars(&cp, &nir).unwrap_or_else(|e| fail(&format!("{cp:?}: {e}")));
        vm::Vm::new(classpath).unwrap_or_else(|e| fail(&format!("the macro VM: {e}")))
    });
    let copies = || sources.iter().map(|s| SourceFile::new(s.name.clone(), s.text.clone())).collect();
    let mut runner = vm.as_mut().map(|vm| VmMacros { vm, sources: copies() });
    let macros = runner.as_mut().map(|r| r as &mut dyn Macros);
    let lowered = match &sources[..] {
        [one] => lower::compile_with(&mut u, one, Config::from_options(&[]), macros),
        many => lower::compile_sources(&mut u, many, Config::from_options(&[]), macros),
    };
    match lowered {
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

fn file_name(file: &str) -> String {
    Path::new(file).file_name().map_or(file.to_owned(), |n| n.to_string_lossy().into_owned())
}

fn read(file: &str) -> String {
    std::fs::read_to_string(file).unwrap_or_else(|e| panic!("reading {file}: {e}"))
}
