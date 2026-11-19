//! What the program writes to its standard output and error where no file descriptor exists
//! (portable mode): kept in the VM until the host takes it.

use crate::Vm;

/// Bytes the program wrote to file descriptor `fd`, 1 or 2, in one or more consecutive writes.
#[derive(Debug, PartialEq, Eq)]
pub struct Written {
    pub fd: i32,
    pub bytes: Vec<u8>,
}

impl Vm {
    /// Takes what the program wrote since the last call, in order; always empty natively, where
    /// writes go to the process's own descriptors.
    pub fn take_output(&mut self) -> Vec<Written> {
        std::mem::take(&mut self.output)
    }

    /// Appends `bytes` written to `fd` to the output.
    pub fn record_output(&mut self, fd: i32, bytes: &[u8]) {
        match self.output.last_mut() {
            Some(last) if last.fd == fd => last.bytes.extend_from_slice(bytes),
            _ => self.output.push(Written { fd, bytes: bytes.to_vec() }),
        }
    }
}
