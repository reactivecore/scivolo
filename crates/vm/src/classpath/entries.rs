//! The NIR files before the library that have entry points (static initializers, externs).

use super::{Classpath, Root};
use nir::prelude::{LENGTH, has_entry_points};
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::io::Read;

/// How many bytes of directory files `entry_point_tops` keeps for `find`; past it, it reads
/// headers only.
const READ_AHEAD: usize = 8 << 20;

impl Classpath {
    /// The top-level names of the NIR files in the directories and jars before the library
    /// whose header says they have entry points, sorted; the first of equal paths counts. A
    /// directory's files are kept for `find` up to `READ_AHEAD` bytes; a jar's file the library
    /// image holds too is judged by the image's copy.
    pub fn entry_point_tops(&mut self) -> Vec<String> {
        let image = self.roots.iter().find_map(|r| match r {
            Root::Image(image) => Some(*image),
            _ => None,
        });
        let (roots, read_ahead) = (&mut self.roots[..self.own], &mut self.read_ahead);
        let mut seen = FxHashSet::default();
        let mut tops: Vec<String> = Vec::new();
        let mut kept = 0;
        super::keep_errno(|| {
            for root in roots {
                let mut names = Vec::new();
                match root {
                    Root::Dir(dir) => super::walk(dir, dir, &mut names),
                    Root::Jar(zip) => names.extend(zip.file_names().flatten().map(Cow::into_owned)),
                    Root::Image(_) => {}
                }
                for name in names {
                    if !name.ends_with(".nir") || !seen.insert(name.clone()) {
                        continue;
                    }
                    let entries = match root {
                        Root::Dir(dir) if kept < READ_AHEAD => {
                            std::fs::read(dir.join(&name)).ok().map(|data| {
                                let entries = has_entry_points(&data[..data.len().min(LENGTH)]);
                                kept += data.len();
                                read_ahead.insert(name.clone(), data);
                                entries
                            })
                        }
                        Root::Dir(dir) => std::fs::File::open(dir.join(&name))
                            .ok()
                            .and_then(head)
                            .map(|h| has_entry_points(&h)),
                        Root::Jar(zip) => match image.and_then(|i| i.find(&name)) {
                            Some((data, _)) => Some(has_entry_points(&data[..data.len().min(LENGTH)])),
                            None => zip.by_name(&name).ok().and_then(head).map(|h| has_entry_points(&h)),
                        },
                        Root::Image(_) => None,
                    };
                    if entries == Some(true) {
                        tops.push(name.trim_end_matches(".nir").replace('/', "."));
                    }
                }
            }
        });
        tops.sort();
        tops
    }
}

/// The first `LENGTH` bytes of `file`.
fn head(file: impl Read) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(LENGTH);
    file.take(LENGTH as u64).read_to_end(&mut out).ok()?;
    Some(out)
}
