//! AST tag table of TastyFormat.scala (Scala 3.9.0, TASTy 28.9), spelled as in the spec.

pub const MAJOR_VERSION: u64 = 28;
pub const MINOR_VERSION: u64 = 9;

pub const UNITconst: u8 = 2;
pub const FALSEconst: u8 = 3;
pub const TRUEconst: u8 = 4;
pub const NULLconst: u8 = 5;
pub const PRIVATE: u8 = 6;
pub const PROTECTED: u8 = 8;
pub const ABSTRACT: u8 = 9;
pub const FINAL: u8 = 10;
pub const SEALED: u8 = 11;
pub const CASE: u8 = 12;
pub const IMPLICIT: u8 = 13;
pub const LAZY: u8 = 14;
pub const OVERRIDE: u8 = 15;
pub const INLINEPROXY: u8 = 16;
pub const INLINE: u8 = 17;
pub const STATIC: u8 = 18;
pub const OBJECT: u8 = 19;
pub const TRAIT: u8 = 20;
pub const ENUM: u8 = 21;
pub const LOCAL: u8 = 22;
pub const SYNTHETIC: u8 = 23;
pub const ARTIFACT: u8 = 24;
pub const MUTABLE: u8 = 25;
pub const FIELDaccessor: u8 = 26;
pub const CASEaccessor: u8 = 27;
pub const COVARIANT: u8 = 28;
pub const CONTRAVARIANT: u8 = 29;
pub const HASDEFAULT: u8 = 31;
pub const STABLE: u8 = 32;
pub const MACRO: u8 = 33;
pub const ERASED: u8 = 34;
pub const OPAQUE: u8 = 35;
pub const EXTENSION: u8 = 36;
pub const GIVEN: u8 = 37;
pub const PARAMsetter: u8 = 38;
pub const EXPORTED: u8 = 39;
pub const OPEN: u8 = 40;
pub const PARAMalias: u8 = 41;
pub const TRANSPARENT: u8 = 42;
pub const INFIX: u8 = 43;
pub const INVISIBLE: u8 = 44;
pub const EMPTYCLAUSE: u8 = 45;
pub const SPLITCLAUSE: u8 = 46;
pub const TRACKED: u8 = 47;
pub const SUBMATCH: u8 = 48;
pub const INTO: u8 = 49;
pub const SHAREDterm: u8 = 60;
pub const SHAREDtype: u8 = 61;
pub const TERMREFdirect: u8 = 62;
pub const TYPEREFdirect: u8 = 63;
pub const TERMREFpkg: u8 = 64;
pub const TYPEREFpkg: u8 = 65;
pub const RECthis: u8 = 66;
pub const BYTEconst: u8 = 67;
pub const SHORTconst: u8 = 68;
pub const CHARconst: u8 = 69;
pub const INTconst: u8 = 70;
pub const LONGconst: u8 = 71;
pub const FLOATconst: u8 = 72;
pub const DOUBLEconst: u8 = 73;
pub const STRINGconst: u8 = 74;
pub const IMPORTED: u8 = 75;
pub const RENAMED: u8 = 76;
pub const THIS: u8 = 90;
pub const QUALTHIS: u8 = 91;
pub const CLASSconst: u8 = 92;
pub const BYNAMEtype: u8 = 93;
pub const BYNAMEtpt: u8 = 94;
pub const NEW: u8 = 95;
pub const THROW: u8 = 96;
pub const IMPLICITarg: u8 = 97;
pub const PRIVATEqualified: u8 = 98;
pub const PROTECTEDqualified: u8 = 99;
pub const RECtype: u8 = 100;
pub const SINGLETONtpt: u8 = 101;
pub const BOUNDED: u8 = 102;
pub const EXPLICITtpt: u8 = 103;
pub const ELIDED: u8 = 104;
pub const IDENT: u8 = 110;
pub const IDENTtpt: u8 = 111;
pub const SELECT: u8 = 112;
pub const SELECTtpt: u8 = 113;
pub const TERMREFsymbol: u8 = 114;
pub const TERMREF: u8 = 115;
pub const TYPEREFsymbol: u8 = 116;
pub const TYPEREF: u8 = 117;
pub const SELFDEF: u8 = 118;
pub const NAMEDARG: u8 = 119;
pub const PACKAGE: u8 = 128;
pub const VALDEF: u8 = 129;
pub const DEFDEF: u8 = 130;
pub const TYPEDEF: u8 = 131;
pub const IMPORT: u8 = 132;
pub const TYPEPARAM: u8 = 133;
pub const PARAM: u8 = 134;
pub const APPLY: u8 = 136;
pub const TYPEAPPLY: u8 = 137;
pub const TYPED: u8 = 138;
pub const ASSIGN: u8 = 139;
pub const BLOCK: u8 = 140;
pub const IF: u8 = 141;
pub const LAMBDA: u8 = 142;
pub const MATCH: u8 = 143;
pub const RETURN: u8 = 144;
pub const WHILE: u8 = 145;
pub const TRY: u8 = 146;
pub const INLINED: u8 = 147;
pub const SELECTouter: u8 = 148;
pub const REPEATED: u8 = 149;
pub const BIND: u8 = 150;
pub const ALTERNATIVE: u8 = 151;
pub const UNAPPLY: u8 = 152;
pub const ANNOTATEDtype: u8 = 153;
pub const ANNOTATEDtpt: u8 = 154;
pub const CASEDEF: u8 = 155;
pub const TEMPLATE: u8 = 156;
pub const SUPER: u8 = 157;
pub const SUPERtype: u8 = 158;
pub const REFINEDtype: u8 = 159;
pub const REFINEDtpt: u8 = 160;
pub const APPLIEDtype: u8 = 161;
pub const APPLIEDtpt: u8 = 162;
pub const TYPEBOUNDS: u8 = 163;
pub const TYPEBOUNDStpt: u8 = 164;
pub const ANDtype: u8 = 165;
pub const ORtype: u8 = 167;
pub const POLYtype: u8 = 169;
pub const TYPELAMBDAtype: u8 = 170;
pub const LAMBDAtpt: u8 = 171;
pub const PARAMtype: u8 = 172;
pub const ANNOTATION: u8 = 173;
pub const TERMREFin: u8 = 174;
pub const TYPEREFin: u8 = 175;
pub const SELECTin: u8 = 176;
pub const EXPORT: u8 = 177;
pub const QUOTE: u8 = 178;
pub const SPLICE: u8 = 179;
pub const METHODtype: u8 = 180;
pub const APPLYsigpoly: u8 = 181;
pub const QUOTEPATTERN: u8 = 182;
pub const SPLICEPATTERN: u8 = 183;
pub const MATCHtype: u8 = 190;
pub const MATCHtpt: u8 = 191;
pub const MATCHCASEtype: u8 = 192;
pub const FLEXIBLEtype: u8 = 193;
pub const HOLE: u8 = 255;

/// Pseudo tag for a name reference stored as a child of METHODtype/POLYtype/TYPELAMBDAtype.
pub const NAMEref: u8 = 0;

pub const FIRST_NAT: u8 = 60;
pub const FIRST_AST: u8 = 90;
pub const FIRST_NAT_AST: u8 = 110;
pub const FIRST_LENGTH: u8 = 128;

/// Constants whose Nat payload is a signed LongInt.
pub fn is_signed_payload(tag: u8) -> bool {
    matches!(tag, BYTEconst | SHORTconst | INTconst | LONGconst | FLOATconst | DOUBLEconst)
}

/// Number of Nat fields following the Length of a category-5 tag.
pub fn leading_nats(tag: u8) -> usize {
    match tag {
        VALDEF | DEFDEF | TYPEDEF | TYPEPARAM | PARAM | RETURN | BIND | REFINEDtype | TERMREFin
        | TYPEREFin | SELECTin | HOLE | SELECTouter => 1,
        PARAMtype => 2,
        _ => 0,
    }
}

pub fn is_methodic(tag: u8) -> bool {
    matches!(tag, POLYtype | TYPELAMBDAtype | METHODtype)
}

pub fn is_modifier(tag: u8) -> bool {
    matches!(tag, 6..=44 | TRACKED | INTO | ANNOTATION | PRIVATEqualified | PROTECTEDqualified)
}

pub fn is_param(tag: u8) -> bool {
    matches!(tag, PARAM | TYPEPARAM | EMPTYCLAUSE | SPLITCLAUSE)
}

pub fn name(tag: u8) -> &'static str {
    match tag {
        NAMEref => "NAMEref",
        UNITconst => "UNITconst",
        FALSEconst => "FALSEconst",
        TRUEconst => "TRUEconst",
        NULLconst => "NULLconst",
        PRIVATE => "PRIVATE",
        PROTECTED => "PROTECTED",
        ABSTRACT => "ABSTRACT",
        FINAL => "FINAL",
        SEALED => "SEALED",
        CASE => "CASE",
        IMPLICIT => "IMPLICIT",
        LAZY => "LAZY",
        OVERRIDE => "OVERRIDE",
        INLINEPROXY => "INLINEPROXY",
        INLINE => "INLINE",
        STATIC => "STATIC",
        OBJECT => "OBJECT",
        TRAIT => "TRAIT",
        ENUM => "ENUM",
        LOCAL => "LOCAL",
        SYNTHETIC => "SYNTHETIC",
        ARTIFACT => "ARTIFACT",
        MUTABLE => "MUTABLE",
        FIELDaccessor => "FIELDaccessor",
        CASEaccessor => "CASEaccessor",
        COVARIANT => "COVARIANT",
        CONTRAVARIANT => "CONTRAVARIANT",
        HASDEFAULT => "HASDEFAULT",
        STABLE => "STABLE",
        MACRO => "MACRO",
        ERASED => "ERASED",
        OPAQUE => "OPAQUE",
        EXTENSION => "EXTENSION",
        GIVEN => "GIVEN",
        PARAMsetter => "PARAMsetter",
        EXPORTED => "EXPORTED",
        OPEN => "OPEN",
        PARAMalias => "PARAMalias",
        TRANSPARENT => "TRANSPARENT",
        INFIX => "INFIX",
        INVISIBLE => "INVISIBLE",
        EMPTYCLAUSE => "EMPTYCLAUSE",
        SPLITCLAUSE => "SPLITCLAUSE",
        TRACKED => "TRACKED",
        SUBMATCH => "SUBMATCH",
        INTO => "INTO",
        SHAREDterm => "SHAREDterm",
        SHAREDtype => "SHAREDtype",
        TERMREFdirect => "TERMREFdirect",
        TYPEREFdirect => "TYPEREFdirect",
        TERMREFpkg => "TERMREFpkg",
        TYPEREFpkg => "TYPEREFpkg",
        RECthis => "RECthis",
        BYTEconst => "BYTEconst",
        SHORTconst => "SHORTconst",
        CHARconst => "CHARconst",
        INTconst => "INTconst",
        LONGconst => "LONGconst",
        FLOATconst => "FLOATconst",
        DOUBLEconst => "DOUBLEconst",
        STRINGconst => "STRINGconst",
        IMPORTED => "IMPORTED",
        RENAMED => "RENAMED",
        THIS => "THIS",
        QUALTHIS => "QUALTHIS",
        CLASSconst => "CLASSconst",
        BYNAMEtype => "BYNAMEtype",
        BYNAMEtpt => "BYNAMEtpt",
        NEW => "NEW",
        THROW => "THROW",
        IMPLICITarg => "IMPLICITarg",
        PRIVATEqualified => "PRIVATEqualified",
        PROTECTEDqualified => "PROTECTEDqualified",
        RECtype => "RECtype",
        SINGLETONtpt => "SINGLETONtpt",
        BOUNDED => "BOUNDED",
        EXPLICITtpt => "EXPLICITtpt",
        ELIDED => "ELIDED",
        IDENT => "IDENT",
        IDENTtpt => "IDENTtpt",
        SELECT => "SELECT",
        SELECTtpt => "SELECTtpt",
        TERMREFsymbol => "TERMREFsymbol",
        TERMREF => "TERMREF",
        TYPEREFsymbol => "TYPEREFsymbol",
        TYPEREF => "TYPEREF",
        SELFDEF => "SELFDEF",
        NAMEDARG => "NAMEDARG",
        PACKAGE => "PACKAGE",
        VALDEF => "VALDEF",
        DEFDEF => "DEFDEF",
        TYPEDEF => "TYPEDEF",
        IMPORT => "IMPORT",
        TYPEPARAM => "TYPEPARAM",
        PARAM => "PARAM",
        APPLY => "APPLY",
        TYPEAPPLY => "TYPEAPPLY",
        TYPED => "TYPED",
        ASSIGN => "ASSIGN",
        BLOCK => "BLOCK",
        IF => "IF",
        LAMBDA => "LAMBDA",
        MATCH => "MATCH",
        RETURN => "RETURN",
        WHILE => "WHILE",
        TRY => "TRY",
        INLINED => "INLINED",
        SELECTouter => "SELECTouter",
        REPEATED => "REPEATED",
        BIND => "BIND",
        ALTERNATIVE => "ALTERNATIVE",
        UNAPPLY => "UNAPPLY",
        ANNOTATEDtype => "ANNOTATEDtype",
        ANNOTATEDtpt => "ANNOTATEDtpt",
        CASEDEF => "CASEDEF",
        TEMPLATE => "TEMPLATE",
        SUPER => "SUPER",
        SUPERtype => "SUPERtype",
        REFINEDtype => "REFINEDtype",
        REFINEDtpt => "REFINEDtpt",
        APPLIEDtype => "APPLIEDtype",
        APPLIEDtpt => "APPLIEDtpt",
        TYPEBOUNDS => "TYPEBOUNDS",
        TYPEBOUNDStpt => "TYPEBOUNDStpt",
        ANDtype => "ANDtype",
        ORtype => "ORtype",
        POLYtype => "POLYtype",
        TYPELAMBDAtype => "TYPELAMBDAtype",
        LAMBDAtpt => "LAMBDAtpt",
        PARAMtype => "PARAMtype",
        ANNOTATION => "ANNOTATION",
        TERMREFin => "TERMREFin",
        TYPEREFin => "TYPEREFin",
        SELECTin => "SELECTin",
        EXPORT => "EXPORT",
        QUOTE => "QUOTE",
        SPLICE => "SPLICE",
        METHODtype => "METHODtype",
        APPLYsigpoly => "APPLYsigpoly",
        QUOTEPATTERN => "QUOTEPATTERN",
        SPLICEPATTERN => "SPLICEPATTERN",
        MATCHtype => "MATCHtype",
        MATCHtpt => "MATCHtpt",
        MATCHCASEtype => "MATCHCASEtype",
        FLEXIBLEtype => "FLEXIBLEtype",
        HOLE => "HOLE",
        _ => "?",
    }
}
