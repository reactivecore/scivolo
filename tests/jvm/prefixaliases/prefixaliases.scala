import scala.language.implicitConversions

object Forward:
  def ended(d: Defs.DefDef[AnyRef]): Defs.DefDef[AnyRef] = d.copied.withMods(1).dropEndMarker()
  def cased(defs: List[Defs.DefDef[AnyRef]]): List[Defs.DefDef[AnyRef]] = defs.map(d => d.withMods(1))
  def inferred(defs: List[Defs.DefDef[AnyRef]]) = defs.map(d => d.withMods(1))

object Defs:
  abstract class Tree[+T <: AnyRef]:
    type ThisTree[T <: AnyRef] <: Tree[T]
    def withMods(mods: Int): ThisTree[AnyRef] = this.asInstanceOf[ThisTree[AnyRef]]

  trait WithEndMarker[+T <: AnyRef] extends Tree[T]:
    final def dropEndMarker(): this.type = this

  case class DefDef[+T <: AnyRef](name: String) extends Tree[T] with WithEndMarker[T]:
    type ThisTree[+T <: AnyRef] = DefDef[T]
    def copied: DefDef[T] = this

abstract class Tree[+T <: AnyRef]:
  type ThisTree[T <: AnyRef] <: Tree[T]
  def withType(tpe: String): ThisTree[String] = this.asInstanceOf[ThisTree[String]]

case class Select[+T <: AnyRef](name: String) extends Tree[T]:
  type ThisTree[+T <: AnyRef] = Select[T]

case class Hole[+T <: AnyRef](idx: Int) extends Tree[T]:
  type ThisTree[+T <: AnyRef] <: Hole[T]

abstract class Name:
  type ThisName <: Name
  def exclude(kind: Int): ThisName
  def mapLast(f: String => String): ThisName
  def ++(other: Name): ThisName = ++(other.toString)
  def ++(other: String): ThisName = mapLast(_ + other)

abstract class TermName extends Name:
  type ThisName = TermName
  override def exclude(kind: Int): TermName = this

final class SimpleName(val s: String) extends TermName:
  def mapLast(f: String => String): SimpleName = SimpleName(f(s))
  override def toString = s

class Node[+T <: AnyRef | Null](val label: String)

object untpd:
  type UNode = Node[AnyRef | Null]

class Sym(val name: String)

abstract class Denotation:
  type AsSeenFromResult <: Denotation
  def asSeenFrom(pre: String): AsSeenFromResult

class SymDenotation(val name: String) extends Denotation:
  type AsSeenFromResult = SymDenotation
  def asSeenFrom(pre: String): SymDenotation = SymDenotation(s"$pre.$name")
  def info: String = name

trait PreDenot:
  type Result <: PreDenot
  def seenFrom(pre: String): Result = computeSeenFrom(pre)
  protected def computeSeenFrom(pre: String): Result
  def filtered(n: Int): PreDenot

abstract class Denot extends PreDenot:
  type Result <: Denot

abstract class SingleDenot(val sig: String) extends Denot:
  def signature: String = sig
  def situated(site: String): String =
    val s = if site == "" then this else seenFrom(site)
    s.signature
  def inherited(pre: String): SingleDenot = filtered(0).seenFrom(pre)
  def filtered(n: Int): SingleDenot = this
  type Result = SingleDenot

class Single(sig: String) extends SingleDenot(sig):
  protected def computeSeenFrom(pre: String): SingleDenot = Single(pre + sig)

object Prefixaliases:
  implicit def toDenot(s: Sym): SymDenotation = SymDenotation(s.name)

  def unstable(tp: String): Select[String] = Select[AnyRef]("a").withType(tp)
  def bounded[S <: Select[AnyRef]](s: S): Select[String] = s.withType("t")
  def hole(h: Hole[AnyRef]): Hole[String] = h.withType("t")
  def setter(name: TermName): TermName = name.exclude(1) ++ "_="
  def chained(name: TermName): TermName =
    val prefix = name ++ "_"
    prefix ++ "a" ++ "b"
  def widened(stat: Node[?]): untpd.UNode = stat
  def converted(s: Sym): String = s.asSeenFrom("pre").info

  def main(args: Array[String]): Unit =
    println(unstable("t"))
    println(bounded(Select[AnyRef]("b")))
    println(hole(Hole(0)))
    println(setter(SimpleName("x")))
    println(chained(SimpleName("x")))
    println(widened(Node[String]("n")).label)
    println(converted(Sym("s")))
    println(Single("s").situated("p."))
    println(Single("t").inherited("q.").signature)
    println(Forward.ended(Defs.DefDef[AnyRef]("d")))
    println(Forward.cased(List(Defs.DefDef("e"))))
    println(Forward.inferred(List(Defs.DefDef("f"))).head.copied.name)
