package dotty.tools.dotc.syntaxoracle

import dotty.tools.dotc.*
import dotty.tools.dotc.ast.{Trees, untpd, desugar, DesugarEnums}
import dotty.tools.dotc.ast.Trees.ApplyKind
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.{Constants, Names}
import dotty.tools.dotc.core.Types.*
import dotty.tools.dotc.util.Spans.Span

/** Untyped trees as S-expressions: `(Class span [@attachments] fields)`, offsets in UTF-8 bytes;
 *  `TypedSplice` prints the referenced symbol, a `DerivedTypeTree` the definition it watches.
 */
class Sexp(bytes: Array[Int], b: StringBuilder):
  import untpd.*

  def at(o: Int): Int = bytes(o min (bytes.length - 1))

  def span(s: Span): String =
    if !s.exists then "-"
    else if s.isSynthetic then s"${at(s.start)}..${at(s.end)}"
    else s"${at(s.start)}..${at(s.end)}@${at(s.point)}"

  def name(n: Names.Name): String = (if n.isTypeName then "T:" else "t:") + Oracle.str(n.toString).drop(1).dropRight(1)

  def const(c: Constants.Constant): String =
    val tag = c.tag match
      case Constants.UnitTag => "Unit"; case Constants.BooleanTag => "Boolean"; case Constants.ByteTag => "Byte"
      case Constants.ShortTag => "Short"; case Constants.CharTag => "Char"; case Constants.IntTag => "Int"
      case Constants.LongTag => "Long"; case Constants.FloatTag => "Float"; case Constants.DoubleTag => "Double"
      case Constants.StringTag => "String"; case Constants.NullTag => "Null"; case Constants.ClazzTag => "Class"
      case t => s"tag$t"
    if c.tag == Constants.UnitTag || c.tag == Constants.NullTag then s"#$tag" else s"#$tag:${Oracle.str(c.stringValue)}"

  def mods(m: Modifiers)(using Context): Unit =
    b ++= "(Mods ["
    b ++= m.flags.flagsString.split(' ').filter(_.nonEmpty).mkString(" ")
    b += ']'
    if !m.privateWithin.isEmpty then b ++= " within=" ++= name(m.privateWithin)
    b ++= " ["; m.annotations.foreach { a => b += ' '; tree(a) }; b ++= "]"
    b ++= " ["; m.mods.foreach { md => b ++= s" (${md.getClass.getSimpleName} ${span(md.span)})" }; b ++= "]"
    b += ')'

  def value(v: Any)(using Context): Unit = v match
    case t: Tree => tree(t)
    case m: Modifiers => mods(m)
    case n: Names.Name => b ++= name(n)
    case c: Constants.Constant => b ++= const(c)
    case l: List[?] => b += '['; l.foreach { x => b += ' '; value(x) }; b ++= "]"
    case s: String => b ++= Oracle.str(s)
    case null => b ++= "null"
    case e: (Boolean | Int) => b ++= e.toString
    case e: scala.reflect.Enum => b ++= e.toString
    case other => b ++= s"<${other.getClass.getSimpleName}>"

  def typeRef(tp: Type)(using Context): String = tp match
    case tp: NamedType => (if tp.isType then "T:" else "t:") + tp.symbol.fullName.toString
    case AppliedType(tycon, args) => typeRef(tycon) + args.map(typeRef).mkString("[", " ", "]")
    case tp: TypeBounds if tp.lo eq tp.hi => typeRef(tp.lo)
    case tp => try tp.show catch case _: Throwable => s"<${tp.getClass.getSimpleName}>"

  def attachments(t: Tree)(using Context): Unit =
    if t.hasAttachment(Trees.Backquoted) then b ++= " @backquoted"
    if t.hasAttachment(Trees.CaptureVar) then b ++= " @capturevar"
    if t.hasAttachment(untpd.RetainsAnnot) then b ++= " @retains"
    if t.hasAttachment(Trees.SyntheticUnit) then b ++= " @syntheticunit"
    if t.hasAttachment(desugar.ForArtifact) then b ++= " @forartifact"
    if t.hasAttachment(desugar.PatternVar) then b ++= " @patternvar"
    if t.hasAttachment(desugar.UntupledParam) then b ++= " @untupled"
    if t.hasAttachment(desugar.ContextBoundParam) then b ++= " @evidence"
    if t.hasAttachment(desugar.TrailingForMap) then b ++= " @trailingmap"
    if t.hasAttachment(desugar.TuplingMigrationForMap) then b ++= " @tuplingmap"
    if t.hasAttachment(desugar.MultiLineInfix) then b ++= " @multiline"
    if t.hasAttachment(desugar.WasTypedInfix) then b ++= " @infix"
    if t.hasAttachment(desugar.PolyFunctionApply) then b ++= " @polyapply"
    if t.hasAttachment(desugar.DerivingCompanion) then b ++= " @deriving"
    if t.hasAttachment(DesugarEnums.DefinesEnumLookupMethods) then b ++= " @enumlookups"
    if t.hasAttachment(transform.SyntheticMembers.ExtendsSingletonMirror) then b ++= " @singletonmirror"
    for m <- t.getAttachment(desugar.CheckIrrefutable) do b ++= s" @check=$m"
    t match
      case t: Apply if t.applyKind != ApplyKind.Regular => b ++= s" @apply=${t.applyKind}"
      case _ =>

  def tree(t: Tree)(using Context): Unit =
    if t eq EmptyTree then { b ++= "_"; return }
    if t eq EmptyValDef then { b ++= "_self"; return }
    if t eq EmptyTypeIdent then { b ++= "_tident"; return }
    val cls = Some(t.getClass.getSimpleName).filter(_.nonEmpty).getOrElse(t.productPrefix)
    b += '(' ++= cls += ' ' ++= span(t.span)
    attachments(t)
    t match
      case t: MemberDef => b += ' '; mods(t.rawMods)
      case t: FunctionWithMods => b += ' '; mods(t.mods); b ++= " erased=" ++= t.erasedParams.mkString("[", " ", "]")
      case t: DerivingTemplate => b ++= s" derived=${t.derived.length}"
      case _ =>
    t match
      case t: DefDef => b += ' '; value(t.name); b += ' '; value(t.paramss); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: ValDef => b += ' '; value(t.name); b += ' '; tree(t.tpt); b += ' '; tree(t.rhs)
      case t: Template => b += ' '; tree(t.constr); b += ' '; value(t.parentsOrDerived); b += ' '; tree(t.self); b += ' '; value(t.body)
      case t: TypedSplice => b += ' ' ++= typeRef(t.splice.tpe)
      case t: DerivedTypeTree =>
        t.watched match
          case w: NameTree => b ++= " ~" ++= name(w.name)
          case _ =>
      case t: Product => t.productIterator.foreach { f => b += ' '; value(f) }
    b += ')'
