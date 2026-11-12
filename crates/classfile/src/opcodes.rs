//! The opcodes code generators emit (JVMS §6.5); a family's first member is named, its others
//! follow it in the order `i l f d a`.

pub const NOP: u8 = 0x00;
pub const ICONST_M1: u8 = 0x02;
pub const ICONST_0: u8 = 0x03;
pub const ICONST_1: u8 = 0x04;
pub const ILOAD: u8 = 0x15;
/// `iload_0`; each kind has `SHORT_VARS` of `xload_n`, in the order of `ILOAD`'s family.
pub const ILOAD_0: u8 = 0x1a;
/// `iaload`; `laload faload daload aaload baload caload saload` follow it.
pub const IALOAD: u8 = 0x2e;
pub const ISTORE: u8 = 0x36;
/// `istore_0`; each kind has `SHORT_VARS` of `xstore_n`, in the order of `ISTORE`'s family.
pub const ISTORE_0: u8 = 0x3b;
/// The number of `xload_n` / `xstore_n` instructions of one kind.
pub const SHORT_VARS: u8 = 4;
/// `iastore`; `lastore fastore dastore aastore bastore castore sastore` follow it.
pub const IASTORE: u8 = 0x4f;
pub const POP: u8 = 0x57;
pub const POP2: u8 = 0x58;
pub const DUP: u8 = 0x59;
pub const IADD: u8 = 0x60;
pub const ISUB: u8 = 0x64;
pub const IMUL: u8 = 0x68;
pub const IDIV: u8 = 0x6c;
pub const IREM: u8 = 0x70;
pub const INEG: u8 = 0x74;
/// `ishl`; the shifts and bitwise operations have an `int` and a `long` member only.
pub const ISHL: u8 = 0x78;
pub const ISHR: u8 = 0x7a;
pub const IUSHR: u8 = 0x7c;
pub const IAND: u8 = 0x7e;
pub const IOR: u8 = 0x80;
pub const IXOR: u8 = 0x82;
pub const I2L: u8 = 0x85;
pub const I2F: u8 = 0x86;
pub const I2D: u8 = 0x87;
pub const L2I: u8 = 0x88;
pub const L2F: u8 = 0x89;
pub const L2D: u8 = 0x8a;
pub const F2I: u8 = 0x8b;
pub const F2L: u8 = 0x8c;
pub const F2D: u8 = 0x8d;
pub const D2I: u8 = 0x8e;
pub const D2L: u8 = 0x8f;
pub const D2F: u8 = 0x90;
/// The conversions between `int`, `long`, `float` and `double`, by source and target in the
/// order `i l f d` (`Kind::offset`); `NOP` where they are equal.
pub const CONVERSIONS: [[u8; 4]; 4] =
    [[NOP, I2L, I2F, I2D], [L2I, NOP, L2F, L2D], [F2I, F2L, NOP, F2D], [D2I, D2L, D2F, NOP]];
pub const I2B: u8 = 0x91;
pub const I2C: u8 = 0x92;
pub const I2S: u8 = 0x93;
pub const LCMP: u8 = 0x94;
/// `fcmpl`; `fcmpg` follows it.
pub const FCMPL: u8 = 0x95;
/// `dcmpl`; `dcmpg` follows it.
pub const DCMPL: u8 = 0x97;
/// `ifeq`; `ifne iflt ifge ifgt ifle` follow it.
pub const IFEQ: u8 = 0x99;
pub const IFNE: u8 = 0x9a;
/// `if_icmpeq`; `if_icmpne if_icmplt if_icmpge if_icmpgt if_icmple` follow it.
pub const IF_ICMPEQ: u8 = 0x9f;
/// `if_acmpeq`; `if_acmpne` follows it.
pub const IF_ACMPEQ: u8 = 0xa5;
pub const GOTO: u8 = 0xa7;
/// `ireturn`; `lreturn freturn dreturn areturn` follow it.
pub const IRETURN: u8 = 0xac;
pub const RETURN: u8 = 0xb1;
pub const NEWARRAY: u8 = 0xbc;
pub const ANEWARRAY: u8 = 0xbd;
pub const ARRAYLENGTH: u8 = 0xbe;
pub const ATHROW: u8 = 0xbf;
pub const WIDE: u8 = 0xc4;
pub const GOTO_W: u8 = 0xc8;
pub const MONITORENTER: u8 = 0xc2;
pub const MONITOREXIT: u8 = 0xc3;
pub const IFNULL: u8 = 0xc6;
pub const IFNONNULL: u8 = 0xc7;
