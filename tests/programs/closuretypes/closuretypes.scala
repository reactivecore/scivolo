package java.closuretypes {
  class Base { def g: Int = 1 }

  class Exec {
    private sealed trait Timed { self: Base =>
      protected var time: Long
      def t: Long = 2L
      def f: Long = t + time + g
      def bump(): Unit = time += 1
    }
    private class Both(var time: Long) extends Base with Timed
    def run(): Long = { val b = new Both(1L); b.bump(); b.f }
  }
}

class Box(val n: Int)

object Pool {
  final def acquire[T](f: Int => T): T = f(41)
}

class Task[A](val a: A) { def run(): Int = 1 }

trait Cons[T] { def accept(t: T): Unit }

trait Coll[A] {
  def get(i: Int): A
  def size: Int
  def each(c: Cons[? >: A]): Unit = { var i = 0; while (i < size) { c.accept(get(i)); i += 1 } }
}

class Early(val b: Box) {
  def this(k: Int) = this(closuretypes.box(k))
  def run(xs: Array[String]): Unit = closuretypes.each(xs)
}

object closuretypes {
  def box(k: Int) =
    Pool.acquire { z =>
      if (k < 0) throw new IllegalArgumentException("k")
      new Box(z + k)
    }

  def each(xs: Array[String]) = {
    xs.foreach { s => print(s) }
    for (i <- 0 until xs.length) print(i)
  }

  def total(ts: Coll[? <: Task[?]]): Int = {
    var i = 0
    var s = 0
    while (i < ts.size) {
      val t = ts.get(i)
      s += t.run() + t.run()
      i += 1
    }
    ts.each { t => s += t.run() }
    s
  }

  def main(args: Array[String]): Unit = {
    val e = new Early(1)
    println(e.b.n)
    e.run(Array("a", "b"))
    println()
    println(new java.closuretypes.Exec().run())
    val c = new Coll[Task[String]] {
      def get(i: Int) = new Task("x")
      def size = 2
    }
    println(total(c))
  }
}
