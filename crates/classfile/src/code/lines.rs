//! Source lines of the code: the `LineNumberTable` attribute.

use super::Code;

impl Code {
    /// Marks the code from here as of source line `line` unless the last mark had it; replaces
    /// a mark at this offset (dotty's `lineNumber`).
    pub fn line(&mut self, line: u16) {
        if line == self.last_line {
            return;
        }
        self.last_line = line;
        let pc = self.bytes.len() as u16;
        match self.lines.last_mut() {
            Some(last) if last.0 == pc => last.1 = line,
            _ => self.lines.push((pc, line)),
        }
    }

    /// The body of the `LineNumberTable` attribute, none without lines.
    pub fn line_table(&self) -> Option<Vec<u8>> {
        if self.lines.is_empty() {
            return None;
        }
        let mut out = (self.lines.len() as u16).to_be_bytes().to_vec();
        for &(pc, line) in &self.lines {
            out.extend(pc.to_be_bytes());
            out.extend(line.to_be_bytes());
        }
        Some(out)
    }
}
