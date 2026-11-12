//! A unit of several source files whose refused files are left out, and with them the files that
//! need them.

use crate::Refusal;
use crate::units::{Attributed, compile_attributed};
use syntax::config::Config;
use syntax::diag::Diagnostics;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::inlining::Macros;
use typer::pickle::Pickled;
use typer::tpd::TNodeId;

/// The output of the files a unit accepts, its warnings and the refusals of the others.
pub struct Isolated<R> {
    pub out: R,
    pub diags: Diagnostics,
    pub refused: Vec<FileRefusal>,
}

/// A source file left out of a unit and why.
pub struct FileRefusal {
    pub file: String,
    pub refusal: Refusal,
}

/// The NIR files of `compile_isolated`.
pub fn compile_sources_isolated(
    open: &mut dyn FnMut() -> Universe,
    files: &[SourceFile],
    cfg: Config,
    macros: Option<&mut dyn Macros>,
) -> Result<Isolated<Vec<crate::NirFile>>, Refusal> {
    let generate = |l: &mut crate::Lower, root, _| crate::codegen::unit(l, root);
    compile_isolated(open, files, cfg, macros, (crate::Backend::Native, generate))
}

/// Compiles `files` as `compile_units` does, each attempt in a universe from `open`; the
/// refusals of an attempt that lie in some files leave them out, blanked so that the others keep
/// their offsets, and the rest is compiled again, so that a file needing a refused one is
/// refused in turn. Refuses the whole unit where it has one file, or no refusal of an attempt
/// lies in a file not left out already.
pub fn compile_isolated<R>(
    open: &mut dyn FnMut() -> Universe,
    files: &[SourceFile],
    cfg: Config,
    mut macros: Option<&mut dyn Macros>,
    (backend, mut generate): (
        crate::Backend,
        impl FnMut(&mut crate::Lower, TNodeId, Vec<Pickled>) -> crate::Result<R>,
    ),
) -> Result<Isolated<R>, Refusal> {
    let mut kept: Vec<SourceFile> = files.iter().map(|f| with_text(f, f.text.clone())).collect();
    let mut left_out = vec![false; files.len()];
    let mut refused = Vec::new();
    loop {
        let mut u = open();
        let generate = (backend, &mut generate);
        let lent = macros.as_deref_mut().map(|m| m as &mut dyn Macros);
        match compile_attributed(&mut u, &kept, cfg, lent, generate) {
            Ok((out, diags)) => return Ok(Isolated { out, diags, refused }),
            Err(mut refusals) => {
                let before = refused.len();
                for Attributed { refusal, files: at } in std::mem::take(&mut refusals) {
                    let at: Vec<usize> = at.into_iter().filter(|&i| !left_out[i]).collect();
                    if at.is_empty() || files.len() == 1 {
                        refusals.push(Attributed { refusal, files: at });
                        continue;
                    }
                    for &i in &at {
                        left_out[i] = true;
                        kept[i] = with_text(&files[i], blank(&files[i].text));
                    }
                    let names = at.iter().map(|&i| files[i].name.clone()).collect::<Vec<_>>().join(", ");
                    refused.push(FileRefusal { file: names, refusal });
                }
                if refused.len() == before {
                    return Err(refusals.swap_remove(0).refusal);
                }
            }
        }
    }
}

fn with_text(f: &SourceFile, text: String) -> SourceFile {
    let mut copy = SourceFile::new(f.name.clone(), text);
    copy.tasty_path.clone_from(&f.tasty_path);
    copy
}

/// `text` with every byte but line breaks a space.
fn blank(text: &str) -> String {
    text.bytes().map(|b| if b == b'\n' { '\n' } else { ' ' }).collect()
}
