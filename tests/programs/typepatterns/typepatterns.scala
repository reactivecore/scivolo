//> using scala 3.9.0
class Animal
class Dog extends Animal:
  def bark: String = "woof"
final class Cat extends Animal

object T:
  def kind(x: Any): String = x match
    case s: String => "string " + s.length
    case i: Int => "int " + (i + 1)
    case _: Dog => "dog"
    case _: Cat => "cat"
    case null => "null"
    case _ => "other"
  def animal(a: Animal): Int = a match
    case _: Dog => 1
    case _: Cat => 2
    case _ => 3
  def same(a: Dog): Int = a match
    case _: Animal => 1
  def guard(x: Any): Int = x match
    case n: Int if n > 10 => n
    case _: Long => 7
    case _: Double => 9
    case _ => 0
  def merged(x: Any): String = x match
    case s: String if s.isEmpty => "empty"
    case s: String => s
    case c: Char => "char " + c
    case b: Boolean => if b then "yes" else "no"
    case _ => "?"
  def tests(x: Any): Boolean = x.isInstanceOf[String] || x.isInstanceOf[Int]
  def dogs(a: Animal): Boolean = a.isInstanceOf[Dog]
  def casts(x: Any): Int = x.asInstanceOf[Int] + 1
  def refs(x: Any): String = x.asInstanceOf[String]

@main def typepatterns(): Unit =
  println(T.kind("abc"))
  println(T.kind(41))
  println(T.kind(Dog()))
  println(T.kind(Cat()))
  println(T.kind(null))
  println(T.kind(1.5))
  println(T.animal(Cat()))
  println(T.same(Dog()))
  println(T.guard(42))
  println(T.guard(7L))
  println(T.guard(2.5))
  println(T.merged(""))
  println(T.merged("s"))
  println(T.merged('c'))
  println(T.merged(false))
  println(T.tests(3))
  println(T.dogs(Cat()))
  println(T.casts(4))
  println(T.refs("r"))
