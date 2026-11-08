//! Reading class files (JVMS §4.1): the class's names, flags and members, attributes skipped.

mod jdk;
mod jimage;

pub use jdk::Jdk;

/// A field or method of a class read: its flags, name and descriptor.
pub struct MemberInfo {
    pub access: u16,
    pub name: String,
    pub descriptor: String,
}

/// A class read from a class file; names are internal names.
pub struct ClassInfo {
    pub access: u16,
    pub name: String,
    pub superclass: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
}

/// The bytes of a class file after an offset.
struct Bytes<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let got = self.bytes.get(self.at..self.at + n).ok_or("a truncated class file")?;
        self.at += n;
        Ok(got)
    }

    fn u2(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u4(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// The constant pool's `Utf8` texts and the `Utf8` index of each `Class` entry, by index.
#[derive(Default)]
struct Constants {
    utf8: Vec<Option<String>>,
    class: Vec<Option<u16>>,
}

impl Constants {
    fn utf8(&self, i: u16) -> Result<String, String> {
        let text = self.utf8.get(usize::from(i)).cloned().flatten();
        text.ok_or_else(|| format!("constant {i} is no Utf8"))
    }

    /// The name of `Class` entry `i`, none for 0.
    fn class(&self, i: u16) -> Result<Option<String>, String> {
        if i == 0 {
            return Ok(None);
        }
        let name = self.class.get(usize::from(i)).copied().flatten();
        self.utf8(name.ok_or_else(|| format!("constant {i} is no Class"))?).map(Some)
    }
}

fn constants(b: &mut Bytes) -> Result<Constants, String> {
    let count = usize::from(b.u2()?);
    let mut c = Constants { utf8: vec![None; count], class: vec![None; count] };
    let mut i = 1;
    while i < count {
        let tag = b.take(1)?[0];
        let skipped = match tag {
            1 => {
                let n = usize::from(b.u2()?);
                c.utf8[i] = Some(String::from_utf8_lossy(b.take(n)?).into_owned());
                0
            }
            7 => {
                c.class[i] = Some(b.u2()?);
                0
            }
            8 | 16 | 19 | 20 => 2,
            15 => 3,
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => 4,
            5 | 6 => 8,
            _ => return Err(format!("constant tag {tag}")),
        };
        b.take(skipped)?;
        i += if matches!(tag, 5 | 6) { 2 } else { 1 };
    }
    Ok(c)
}

fn skip_attributes(b: &mut Bytes) -> Result<(), String> {
    for _ in 0..b.u2()? {
        b.u2()?;
        let n = b.u4()? as usize;
        b.take(n)?;
    }
    Ok(())
}

fn members(b: &mut Bytes, c: &Constants) -> Result<Vec<MemberInfo>, String> {
    (0..b.u2()?)
        .map(|_| {
            let access = b.u2()?;
            let (name, descriptor) = (c.utf8(b.u2()?)?, c.utf8(b.u2()?)?);
            skip_attributes(b)?;
            Ok(MemberInfo { access, name, descriptor })
        })
        .collect()
}

/// The class of class file `bytes`.
pub fn read(bytes: &[u8]) -> Result<ClassInfo, String> {
    let mut b = Bytes { bytes, at: 0 };
    if b.u4()? != 0xCAFE_BABE {
        return Err("no class file".into());
    }
    b.take(4)?;
    let c = constants(&mut b)?;
    let access = b.u2()?;
    let name = c.class(b.u2()?)?.ok_or("a class without a name")?;
    let superclass = c.class(b.u2()?)?;
    let interfaces = (0..b.u2()?).map(|_| c.class(b.u2()?)?.ok_or_else(|| "interface 0".into()));
    let interfaces = interfaces.collect::<Result<_, String>>()?;
    let fields = members(&mut b, &c)?;
    let methods = members(&mut b, &c)?;
    Ok(ClassInfo { access, name, superclass, interfaces, fields, methods })
}
