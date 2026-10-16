//> using scala 3.9.0
import scala.reflect.ClassTag

class Catcher[E <: Throwable]:
  def attempt[A](a: => A): E | A =
    try a
    catch case e: E => e

def tagged[T: ClassTag](x: Any): Boolean = x match
  case _: T => true
  case _ => false

def taggedBind[T: ClassTag](x: Any): String = x match
  case t: T => s"T $t"
  case _ => "other"

def bounded[T <: CharSequence](x: CharSequence): String = x match
  case t: T => s"T $t"

type Text = String

def aliased(x: Any): String = x match
  case t: Text => s"text $t"
  case _ => "other"

@main def abstractpatterns(): Unit =
  println(Catcher[ArithmeticException]().attempt(1 / 0))
  println(Catcher[Throwable]().attempt(6 * 7))
  println(tagged[String]("a"))
  println(tagged[String](1))
  println(tagged[Int](1))
  println(taggedBind[String]("d"))
  println(taggedBind[Integer](2.5))
  println(bounded[String]("b"))
  println(aliased("c"))
  println(aliased(3))
