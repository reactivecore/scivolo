//! The positions and symbol names of `quotes.reflect`: a position is the number of its source
//! file and its two offsets in that file.

use crate::host::Host;
use crate::reflect::symbol;
use syntax::ast::Span;
use syntax::source::Sources;
use vm::Reflected;

type Answer = Result<Reflected, &'static str>;

impl Host {
    /// Runs position or symbol operation `op` on `args`.
    pub fn run_position(&self, op: &str, args: &[i32]) -> Answer {
        let arg = |i: usize| args.get(i).copied().ok_or("a quotes.reflect operation without its arguments");
        let file = |n: i32| self.sources.get(n as usize).ok_or("a position in no source file");
        let text = |s: String| Ok((Vec::new(), Some(s)));
        match op {
            "expansionPos" => self.located(self.expansion),
            "pos" => self.located(self.tpd.span(self.tree(arg(0)? as u32)?)),
            "position" => {
                let f = file(arg(0)?)?;
                let (start, end) = (arg(1)? as u32, arg(2)? as u32);
                let at = |o| [f.offset_to_line(o) as i32, f.column(o) as i32];
                let code = f.text.get(start as usize..end as usize).map(str::to_owned);
                Ok(([at(start), at(end)].concat(), code))
            }
            "sourcePath" => text(file(arg(0)?)?.name.clone()),
            "sourceContent" => text(file(arg(0)?)?.text.clone()),
            "spliceOwner" => Ok((vec![self.owner.0 as i32], None)),
            "owner" => {
                let owner = self.u.symbol(symbol(arg(0)?)?).owner;
                Ok((vec![owner.map_or(-1, |o| o.0 as i32)], None))
            }
            "name" => {
                let s = symbol(arg(0)?)?;
                let name = self.u.symbol(s).name.to_string();
                match name.is_empty() {
                    true if s == self.u.root() => text("<root>".into()),
                    true => text("<empty>".into()),
                    false => text(name),
                }
            }
            "fullName" => text(self.u.full_name(symbol(arg(0)?)?)),
            _ => Err("a quotes.reflect operation the frontend does not know"),
        }
    }

    /// `span` as the number of its source file and its offsets in that file.
    fn located(&self, span: Span) -> Answer {
        if !span.exists() || span.is_in_other_source() || self.sources.is_empty() {
            return Err("a position outside the sources of the unit");
        }
        let span = span.without_inlined_call();
        let sources = Sources::new(&self.sources).map_err(|_| "sources of more than 1 GiB")?;
        let i = sources.index(span.start());
        let base = sources.files()[i].0;
        Ok((vec![i as i32, (span.start() - base) as i32, (span.end() - base) as i32], None))
    }
}
