import scala.language.implicitConversions

abstract class Tree[+T <: AnyRef]:
  type ThisTree[T <: AnyRef] <: Tree[T]
  def withType(tpe: String): ThisTree[String] = this.asInstanceOf[ThisTree[String]]

case class Select[+T <: AnyRef](name: String) extends Tree[T]:
  type ThisTree[+T <: AnyRef] = Select[T]

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

object Prefixaliases:
  implicit def toDenot(s: Sym): SymDenotation = SymDenotation(s.name)

  def unstable(tp: String): Select[String] = Select[AnyRef]("a").withType(tp)
  def bounded[S <: Select[AnyRef]](s: S): Select[String] = s.withType("t")
  def setter(name: TermName): TermName = name.exclude(1) ++ "_="
  def widened(stat: Node[?]): untpd.UNode = stat
  def converted(s: Sym): String = s.asSeenFrom("pre").info

  def main(args: Array[String]): Unit =
    println(unstable("t"))
    println(bounded(Select[AnyRef]("b")))
    println(setter(SimpleName("x")))
    println(widened(Node[String]("n")).label)
    println(converted(Sym("s")))
