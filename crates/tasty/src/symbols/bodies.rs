//! What reading method bodies needs of a loaded file beyond its symbols: the spans of its trees
//! and the signatures of its signed names, decoded from the file again on first use.

use super::{Error, FileId, Universe};
use crate::Tasty;
use crate::names::{Name, NameRef, ParamSig};
use crate::positions::{TastySpan, decode_positions};
use rustc_hash::FxHashMap;

pub struct BodyInfo {
    spans: FxHashMap<u32, TastySpan>,
    /// The parameter part of each signed name (dotty's `Signature.paramsSig`): the number of
    /// type parameters of a clause, or the erased class name of a value parameter.
    signatures: FxHashMap<u32, Vec<String>>,
}

impl BodyInfo {
    /// The span pickled for the tree at address `addr`.
    pub fn span(&self, addr: u32) -> Option<TastySpan> {
        self.spans.get(&addr).copied()
    }

    /// The parameter signature of name `name`, if it is a signed name.
    pub fn signature(&self, name: NameRef) -> Option<&[String]> {
        self.signatures.get(&name.0).map(Vec::as_slice)
    }
}

impl Universe {
    /// The spans and signatures of loaded file `file`.
    pub fn body_info(&mut self, file: FileId) -> Result<&BodyInfo, Error> {
        let loaded = &mut self.files[file.0 as usize];
        if loaded.body_info.is_none() {
            let info = BodyInfo::decode(&loaded.bytes).map_err(|e| Error::Corrupt(loaded.path.clone(), e))?;
            loaded.body_info = Some(info);
        }
        Ok(loaded.body_info.as_ref().expect("decoded"))
    }
}

impl BodyInfo {
    fn decode(bytes: &[u8]) -> crate::Result<BodyInfo> {
        let tasty = Tasty::parse(bytes)?;
        let spans = match tasty.section("Positions") {
            Some(s) => decode_positions(s.bytes)?,
            None => FxHashMap::default(),
        };
        let signatures = tasty
            .names
            .iter()
            .filter_map(|(r, n)| match n {
                Name::Signed { params, .. } => {
                    let shown = params.iter().map(|p| match p {
                        ParamSig::TypeParams(n) => n.to_string(),
                        ParamSig::Term(t) => tasty.names.show(*t),
                    });
                    Some((r.0, shown.collect()))
                }
                _ => None,
            })
            .collect();
        Ok(BodyInfo { spans, signatures })
    }
}
