package annotlib

class tagged(n: Int) extends scala.annotation.StaticAnnotation
class mark extends scala.annotation.StaticAnnotation
class named(s: String, k: Int = 2) extends scala.annotation.StaticAnnotation
class dep(x: Any) extends scala.annotation.StaticAnnotation

object K { final val N = 7; def m = 3 }

class Lib {
  def tags: List[String @tagged(3)] = Nil
  def fives: List[String @tagged(5)] = Nil
  def threes: Option[String @tagged(3)] = None
  def marks: List[Int @mark] = Nil
  def both(x: Long @tagged(4)): Option[String @named("a")] = None
  val pair: (Int @tagged(1 + 2), String @mark) = (1, "a")
}

class Box[A](a: A) {
  def marked: List[A @mark] = Nil
  def tagged: Option[A @tagged(K.N)] = None
  def static: List[Char @dep(K.m)] = Nil
  def full: List[Short @scala.annotation.unchecked.uncheckedVariance] = Nil
  def dependent(x: Int): List[Boolean @dep(x)] = Nil
}
