// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/PickleFormat.scala
//! The entry tags and versions of Scala 2 pickles.
#![allow(non_upper_case_globals, dead_code)]

pub const MAJOR_VERSION: usize = 5;
pub const MINOR_VERSION: usize = 2;

pub const TERMname: u8 = 1;
pub const TYPEname: u8 = 2;
pub const NONEsym: u8 = 3;
pub const TYPEsym: u8 = 4;
pub const ALIASsym: u8 = 5;
pub const CLASSsym: u8 = 6;
pub const MODULEsym: u8 = 7;
pub const VALsym: u8 = 8;
pub const EXTref: u8 = 9;
pub const EXTMODCLASSref: u8 = 10;
pub const NOtpe: u8 = 11;
pub const NOPREFIXtpe: u8 = 12;
pub const THIStpe: u8 = 13;
pub const SINGLEtpe: u8 = 14;
pub const CONSTANTtpe: u8 = 15;
pub const TYPEREFtpe: u8 = 16;
pub const TYPEBOUNDStpe: u8 = 17;
pub const REFINEDtpe: u8 = 18;
pub const CLASSINFOtpe: u8 = 19;
pub const METHODtpe: u8 = 20;
pub const POLYtpe: u8 = 21;
pub const IMPLICITMETHODtpe: u8 = 22;
pub const LITERALunit: u8 = 24;
pub const LITERALboolean: u8 = 25;
pub const LITERALbyte: u8 = 26;
pub const LITERALshort: u8 = 27;
pub const LITERALchar: u8 = 28;
pub const LITERALint: u8 = 29;
pub const LITERALlong: u8 = 30;
pub const LITERALfloat: u8 = 31;
pub const LITERALdouble: u8 = 32;
pub const LITERALstring: u8 = 33;
pub const LITERALnull: u8 = 34;
pub const LITERALclass: u8 = 35;
pub const LITERALenum: u8 = 36;
pub const SYMANNOT: u8 = 40;
pub const CHILDREN: u8 = 41;
pub const ANNOTATEDtpe: u8 = 42;
pub const ANNOTINFO: u8 = 43;
pub const ANNOTARGARRAY: u8 = 44;
pub const SUPERtpe: u8 = 46;
pub const DEBRUIJNINDEXtpe: u8 = 47;
pub const EXISTENTIALtpe: u8 = 48;
pub const TREE: u8 = 49;
pub const MODIFIERS: u8 = 50;

/// The name of refinement classes (dotty's `tpnme.REFINE_CLASS`).
pub const REFINE_CLASS: &str = "<refinement>";

/// Describes the type of tag `tag` that this reader refuses, for a refusal.
pub fn refused_type(tag: u8) -> String {
    match tag {
        REFINEDtpe => "a refinement type".into(),
        EXISTENTIALtpe => "an existential type".into(),
        ANNOTATEDtpe => "an annotated type".into(),
        SUPERtpe => "a super type".into(),
        DEBRUIJNINDEXtpe => "a De Bruijn index type".into(),
        POLYtpe => "a type lambda".into(),
        METHODtpe | IMPLICITMETHODtpe => "a method type".into(),
        _ => format!("a type of tag {tag}"),
    }
}
