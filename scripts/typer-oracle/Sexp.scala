package dotty.tools.dotc.typeroracle

import dotty.tools.dotc.ast.tpd.*
import dotty.tools.dotc.core.Constants
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.Flags.*
import dotty.tools.dotc.core.Names.Name
import dotty.tools.dotc.core.Symbols.*
import dotty.tools.dotc.core.Types.*
import dotty.tools.dotc.util.Spans.Span

/** Typed trees as S-expressions: `(Class span [sym=…] [[flags]] [:type] fields)`, offsets in UTF-8
 *  bytes, symbols as their full name and, for methods, their erased parameter types. */
class Sexp(bytes: Array[Int], b: StringBuilder)(using Context):
  def at(o: Int): Int = bytes(o min (bytes.length - 1))

  def span(s: Span): String =
    if !s.exists then "-"
    else if s.isSynthetic then s"${at(s.start)}..${at(s.end)}"
    else s"${at(s.start)}..${at(s.end)}@${at(s.point)}"

  def name(n: Name): String = (if n.isTypeName then "T:" else "t:") + Oracle.str(n.toString).drop(1).dropRight(1)

  def const(c: Constants.Constant): String =
    import Constants.*
    val tag = c.tag match
      case UnitTag => "Unit"; case BooleanTag => "Boolean"; case ByteTag => "Byte"; case ShortTag => "Short"
      case CharTag => "Char"; case IntTag => "Int"; case LongTag => "Long"; case FloatTag => "Float"
      case DoubleTag => "Double"; case StringTag => "String"; case NullTag => "Null"; case ClazzTag => "Class"
      case t => s"tag$t"
    if c.tag == UnitTag || c.tag == NullTag then s"#$tag"
    else if c.tag == ClazzTag then s"#Class:${tpe(c.typeValue, 0)}"
    else s"#$tag:${Oracle.str(c.stringValue)}"

  def sym(s: Symbol): String =
    if !s.exists then "<none>"
    else
      val sig =
        if s.is(Method) && !s.isConstructor then
          try s.signature.paramsSig.map(_.toString).mkString("#(", ",", ")") catch case _: Throwable => "#?"
        else ""
      Oracle.str(s.fullName.toString + sig)

  /** The TASTy modifier tags `TreePickler.pickleFlags` writes for `s`, by tag number. */
  def flags(s: Symbol): String =
    import dotty.tools.tasty.TastyFormat.*
    val f = s.flags
    val common = List(Private -> PRIVATE, Protected -> PROTECTED, Case -> CASE, Override -> OVERRIDE,
      Inline -> INLINE, InlineProxy -> INLINEPROXY, Macro -> MACRO, JavaStatic -> STATIC, Module -> OBJECT,
      Enum -> ENUM, Local -> LOCAL, Synthetic -> SYNTHETIC, Artifact -> ARTIFACT, Transparent -> TRANSPARENT,
      Infix -> INFIX, Invisible -> INVISIBLE, Erased -> ERASED, Exported -> EXPORTED, Given -> GIVEN,
      Implicit -> IMPLICIT, Tracked -> TRACKED)
    val term = List(Mutable -> MUTABLE, Accessor -> FIELDaccessor, CaseAccessor -> CASEaccessor,
      HasDefault -> HASDEFAULT, Extension -> EXTENSION, ParamAccessor -> PARAMsetter, SuperParamAlias -> PARAMalias)
    val tpe = List(Sealed -> SEALED, Abstract -> ABSTRACT, Trait -> TRAIT, Covariant -> COVARIANT,
      Contravariant -> CONTRAVARIANT, Opaque -> OPAQUE, Open -> OPEN, Into -> INTO)
    val tags = common.collect { case (flag, tag) if f.is(flag) => tag } ++
      (if f.is(Final, butNot = Module) then List(FINAL) else Nil) ++
      (if s.isTerm then
        term.collect { case (flag, tag) if f.is(flag) => tag } ++
          (if f.is(Lazy, butNot = Module) then List(LAZY) else Nil) ++
          (if f.is(AbsOverride) then List(ABSTRACT, OVERRIDE) else Nil) ++
          (if f.isAllOf(StableMethod) then List(STABLE) else Nil)
      else tpe.collect { case (flag, tag) if f.is(flag) => tag })
    tags.distinct.sorted.map(astTagToString).mkString(" ")

  def params(ns: List[Name], is: List[Type], d: Int): String =
    ns.zip(is).map((n, i) => s"$n${if i.isInstanceOf[TypeBounds] then " " else ": "}${tpe(i, d)}").mkString(", ")

  def tpe(t: Type, d: Int): String =
    if d > 14 then return "…"
    val n = d + 1
    try t match
      case t: NamedType =>
        val s = t.symbol
        val k = if t.isType then "T:" else "t:"
        def full = if s.exists then s.fullName.toString else t.name.toString
        t.prefix match
          case NoPrefix => k + full
          case p: ThisType if p.cls.is(Package) || p.cls.is(ModuleClass) => k + full
          case p: TermRef if p.symbol.is(Package) => k + full
          case p => s"${tpe(p, n)}.$k${t.name}"
      case t: ThisType => tpe(t.tref, n) + ".this"
      case SuperType(thistpe, supertpe) => s"${tpe(thistpe, n)}.super[${tpe(supertpe, n)}]"
      case ConstantType(c) => const(c)
      case t: LazyRef => tpe(t.ref, n)
      case RefinedType(parent, nm, info) => s"${tpe(parent, n)}{$nm ${tpe(info, n)}}"
      case t: RecType => s"rec{${tpe(t.parent, n)}}"
      case AppliedType(tycon, args) => tpe(tycon, n) + args.map(tpe(_, n)).mkString("[", " ", "]")
      case t: TypeParamRef => s"${t.paramName}#${t.paramNum}"
      case t: TermParamRef => s"${t.paramName}"
      case _: RecThis => "rec.this"
      case t: SkolemType => s"?${tpe(t.info, n)}"
      case t: TypeVar => if t.isInstantiated then tpe(t.instanceOpt, n) else s"?tv(${tpe(t.origin, n)})"
      case ExprType(res) => s"=> ${tpe(res, n)}"
      case t: MethodType =>
        (if t.isContextualMethod then "using " else if t.isImplicitMethod then "implicit " else "") +
          s"(${params(t.paramNames, t.paramInfos, n)})${tpe(t.resType, n)}"
      case t: PolyType => s"[${params(t.paramNames, t.paramInfos, n)}]${tpe(t.resType, n)}"
      case t: HKTypeLambda => s"[${params(t.paramNames, t.paramInfos, n)}] =>> ${tpe(t.resType, n)}"
      case TypeAlias(alias) => "= " + tpe(alias, n)
      case TypeBounds(lo, hi) => s">: ${tpe(lo, n)} <: ${tpe(hi, n)}"
      case AnnotatedType(parent, annot) => s"${tpe(parent, n)} @${annot.symbol.fullName}"
      case AndType(a, c) => s"(${tpe(a, n)} & ${tpe(c, n)})"
      case OrType(a, c) => s"(${tpe(a, n)} | ${tpe(c, n)})"
      case t: MatchType => s"(${tpe(t.scrutinee, n)} match {${t.cases.map(tpe(_, n)).mkString("; ")}} <: ${tpe(t.bound, n)})"
      case t: FlexibleType => s"${tpe(t.hi, n)}?"
      case _: ClassInfo => "<class>"
      case NoType => "<notype>"
      case NoPrefix => "<noprefix>"
      case t: WildcardType => "?" + (if t.optBounds.exists then tpe(t.optBounds, n) else "")
      case _: ErrorType => "<error>"
      case t => s"<${t.getClass.getSimpleName}>"
    catch case e: Throwable => s"<${e.getClass.getSimpleName}>"

  def value(v: Any): Unit = v match
    case t: Tree => tree(t)
    case n: Name => b ++= name(n)
    case c: Constants.Constant => b ++= const(c)
    case l: List[?] => b += '['; l.foreach { x => b += ' '; value(x) }; b ++= "]"
    case s: String => b ++= Oracle.str(s)
    case null => b ++= "null"
    case e: (Boolean | Int) => b ++= e.toString
    case e: scala.reflect.Enum => b ++= e.toString
    case t: Type => b ++= tpe(t, 0)
    case other => b ++= s"<${other.getClass.getSimpleName}>"

  def tree(t: Tree): Unit =
    if t eq EmptyTree then { b ++= "_"; return }
    if t eq EmptyValDef then { b ++= "_self"; return }
    val cls = Some(t.getClass.getSimpleName).filter(_.nonEmpty).getOrElse(t.productPrefix)
    b += '(' ++= cls += ' ' ++= span(t.span)
    t match
      case t: (RefTree | DefTree | This | Super | New) if t.symbol.exists => b ++= " sym=" ++= sym(t.symbol)
      case _ =>
    t match
      case t: MemberDef => b ++= " [" ++= flags(t.symbol) ++= "]"
      case _ =>
    if t.hasType && (t.isTerm || t.isType) then b ++= " :" ++= tpe(t.tpe, 0)
    t match
      case t: DefDef => b += ' '; value(t.name); b += ' '; value(t.paramss); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: ValDef => b += ' '; value(t.name); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: Template => b += ' '; tree(t.constr); b += ' '; value(t.parents); b += ' '; tree(t.self); b += ' '; value(t.body)
      case _: TypeTree => ()
      case t: Product => t.productIterator.foreach { f => b += ' '; value(f) }
    b += ')'
