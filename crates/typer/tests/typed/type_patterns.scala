class Animal { def name = "a" }
class Dog extends Animal { def bark = "w" }
class Cat extends Animal

object Main:
  def f(x: Any) = x match
    case d: Dog => d.bark
    case _ => ""
  def g(a: Animal) = a match
    case c: Cat => 2
    case _ => 3
  def h(a: Dog) = a match
    case d: Animal => d.bark + d.name
  def k(x: Any): Int = x match
    case m: Long => m.toInt
    case s: String if s.nonEmpty => s.length
    case _ => 0
  def run(): Int =
    println(f(Dog()) + h(Dog()))
    g(Cat()) + k(3L) + k("ab")
