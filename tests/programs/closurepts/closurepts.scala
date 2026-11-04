abstract class Foo[T]:
  type Species
  def name: String = "foo"

class NK:
  class I
  type T
  def tag: String = "nk"

object closurepts:
  def orNull[T](x: T | Null): T = x.asInstanceOf[T]
  def both[T](x: T | T): T = x

  def species[T] =
    class FooT extends Foo[T]:
      type Species = FooT
      override def name = "fooT"
    new FooT()

  val inner =
    class C extends NK:
      type T = I
      override def tag = "c"
    new C

  def main(args: Array[String]): Unit =
    val g: String => Int = orNull { s => s.length }
    val h: String => Int = both { s => s.length + 1 }
    println(g("abc") + h("ab"))
    val l = List(identity[Int](_))
    println(l.head(7))
    for (a, b) <- List("x", "y").lazyZip(List(1, 2)) do println(s"$a$b")
    val pairs: Option[(Int, Int) => Int] = Option { case (a, b) => a * b }
    println(pairs.get(6, 7))
    println(species[Int].name)
    println(inner.tag)
    val projected: (NK { type T = Int })#T = 5
    println(projected + 1)
