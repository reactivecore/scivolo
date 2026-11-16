package scala.probe

class tg[T] extends scala.annotation.StaticAnnotation
class tv[T](x: List[T]) extends scala.annotation.StaticAnnotation

trait TgSizes[A] {
  def size(that: List[A] @tg[A]): Int = that.length
  def sum(that: Int @tv[A](Nil)): Int = that
}

trait TgLengths extends TgSizes[String] {
  override def size(that: List[String] @tg[String]): Int = 0
  def len(that: List[String] @tg[String]): Int = super.size(that)
  def total(that: Int): Int = super.sum(that)
}

trait TgGeneric[B] extends TgSizes[B] {
  override def size(that: List[B] @tg[B]): Int = 0
  def len(that: List[B] @tg[B]): Int = super.size(that)
}
