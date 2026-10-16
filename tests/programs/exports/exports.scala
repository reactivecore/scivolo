class Counter(start: Int):
  val base: Int = start
  def add(n: Int, step: Int = 1): Int = base + n * step
  def sum[T](xs: T*)(f: T => Int): Int = xs.map(f).sum
  type Item = String
  given label: String = "counter"
  extension (s: String) def shout: String = s.toUpperCase

class Wrapper(c: Counter):
  export c.{add as plus, sum, Item}

object Facade:
  val counter = Counter(10)
  export counter.{given, *}

object Ints:
  class Box(x: Int):
    def twice: Int = x * 2
    def plus(y: Int): Int = x + y
  extension (x: Int)
    private def box = Box(x)
    export box.{twice, plus}

@main def exports(): Unit =
  val w = Wrapper(Counter(5))
  println(w.plus(2))
  println(w.plus(2, 3))
  println(w.sum(1, 2, 3)(_ * 2))
  val item: w.Item = "item"
  println(item)
  println(Facade.base)
  println(Facade.add(1))
  println(Facade.label)
  import Facade.shout
  println("hi".shout)
  import Ints.*
  println(3.twice)
  println(3.plus(4))
