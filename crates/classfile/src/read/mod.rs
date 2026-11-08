//! Reading class files (JVMS §4.1): the class's names, flags, members and `Signature`
//! attributes, other attributes skipped.

mod jdk;
mod jimage;

pub use jdk::Jdk;

/// A field or method of a class read: its flags, name, descriptor, generic signature and a
/// field's constant value.
#[derive(Clone)]
pub struct MemberInfo {
    pub access: u16,
    pub name: String,
    pub descriptor: String,
    pub signature: Option<String>,
    pub constant: Option<ConstantValue>,
}

/// A class read from a class file; names are internal names.
#[derive(Clone)]
pub struct ClassInfo {
    pub access: u16,
    pub name: String,
    pub superclass: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
    pub signature: Option<String>,
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

/// The value of a field's `ConstantValue` attribute (JVMS §4.7.2); floating-point values by
/// their bits.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstantValue {
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    String(String),
}

/// The constant pool's `Utf8` texts with whether each is exact, the `Utf8` index of each `Class`
/// entry and the values of the entries a `ConstantValue` may name, by index; a `String`'s as its
/// `Utf8` index.
#[derive(Default)]
struct Constants {
    utf8: Vec<Option<(String, bool)>>,
    class: Vec<Option<u16>>,
    values: Vec<Option<(u8, u64)>>,
}

impl Constants {
    fn value(&self, i: u16) -> Result<ConstantValue, String> {
        let Some(&Some((tag, bits))) = self.values.get(usize::from(i)) else {
            return Err(format!("constant {i} is no value"));
        };
        Ok(match tag {
            3 => ConstantValue::Int(bits as u32 as i32),
            4 => ConstantValue::Float(bits as u32),
            5 => ConstantValue::Long(bits as i64),
            6 => ConstantValue::Double(bits),
            _ => match self.utf8.get(bits as usize) {
                Some(Some((s, true))) => ConstantValue::String(s.clone()),
                _ => return Err(format!("constant {i} is no exact String")),
            },
        })
    }

    fn utf8(&self, i: u16) -> Result<String, String> {
        let text = self.utf8.get(usize::from(i)).cloned().flatten();
        text.map(|(s, _)| s).ok_or_else(|| format!("constant {i} is no Utf8"))
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
    let mut c = Constants { utf8: vec![None; count], class: vec![None; count], values: vec![None; count] };
    let mut i = 1;
    while i < count {
        let tag = b.take(1)?[0];
        let skipped = match tag {
            1 => {
                let n = usize::from(b.u2()?);
                c.utf8[i] = Some(crate::modified_utf8::decode(b.take(n)?));
                0
            }
            7 => {
                c.class[i] = Some(b.u2()?);
                0
            }
            3 | 4 => {
                c.values[i] = Some((tag, u64::from(b.u4()?)));
                0
            }
            5 | 6 => {
                c.values[i] = Some((tag, u64::from(b.u4()?) << 32 | u64::from(b.u4()?)));
                0
            }
            8 => {
                c.values[i] = Some((tag, u64::from(b.u2()?)));
                0
            }
            16 | 19 | 20 => 2,
            15 => 3,
            9 | 10 | 11 | 12 | 17 | 18 => 4,
            _ => return Err(format!("constant tag {tag}")),
        };
        b.take(skipped)?;
        i += if matches!(tag, 5 | 6) { 2 } else { 1 };
    }
    Ok(c)
}

/// The `Signature` (JVMS §4.7.9) and `ConstantValue` attributes among the attributes, the
/// others skipped.
fn attributes(b: &mut Bytes, c: &Constants) -> Result<(Option<String>, Option<ConstantValue>), String> {
    let (mut signature, mut constant) = (None, None);
    for _ in 0..b.u2()? {
        let name = b.u2()?;
        let n = b.u4()? as usize;
        let mut value = Bytes { bytes: b.take(n)?, at: 0 };
        match c.utf8(name).as_deref() {
            Ok("Signature") if n == 2 => signature = Some(c.utf8(value.u2()?)?),
            Ok("ConstantValue") if n == 2 => constant = Some(c.value(value.u2()?)?),
            _ => {}
        }
    }
    Ok((signature, constant))
}

fn members(b: &mut Bytes, c: &Constants) -> Result<Vec<MemberInfo>, String> {
    (0..b.u2()?)
        .map(|_| {
            let access = b.u2()?;
            let (name, descriptor) = (c.utf8(b.u2()?)?, c.utf8(b.u2()?)?);
            let (signature, constant) = attributes(b, c)?;
            Ok(MemberInfo { access, name, descriptor, signature, constant })
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
    let (signature, _) = attributes(&mut b, &c)?;
    Ok(ClassInfo { access, name, superclass, interfaces, fields, methods, signature })
}
