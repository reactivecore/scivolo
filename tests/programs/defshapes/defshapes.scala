//> using scala 3.9.0
// Constructors with dependent bounds, local aliases, private classes, self types, anonymous classes
// with a self name, annotation aliases.

package ds {
  import scala.annotation.StaticAnnotation
  import Pool.Queue.slot

  import Pool._

  trait It[E] { def next(): E }
  class One[E](e: E) extends It[E] { def next(): E = e }
  class Wrapped[E, Iter <: It[E]](val inner: Iter) { def next(): E = inner.next() }

  class HM[K] { private final class Iter; def get: Int = 1 }
  object HM { private[ds] class Node[K](val k: K) }
  class LHM[K] extends HM[K] { private final class Iter; def n: Int = 2 }
  object LHM { private final class Node[K](k: K) extends HM.Node[K](k) }

  class Task[V](val v: V)
  trait Scheduled[V <: AnyRef] { self: Task[V] =>
    var outer: Task[V] = this
  }

  class ListView(val from: Int, val to: Int) { def size: Int = to - from }

  class Marker(s: String) extends StaticAnnotation

  class Pool {
    @Tag("a") var ctl: Long = 4
  }

  object Pool {
    private type Tag = Marker
    final class Queue {
      @Tag("w") var top: Int = 5
    }
    object Queue { def slot: Int = 1 }
  }

  object Shapes {
    def built(): String = {
      type A = java.lang.StringBuilder
      val f = () => new A
      f().append("built").toString
    }

    def view(n: Int): ListView =
      new ListView(0, n) with java.util.RandomAccess {
        selfView =>
        override def size: Int = selfView.to + 1
      }

    def run(): Unit = {
      val w = new Wrapped(new One("wrapped"))
      println(w.next())
      println(new LHM[String]().get + new LHM[String]().n)
      val t = new Task[String]("task") with Scheduled[String]
      println(t.outer.v)
      println(view(3).size)
      println(new Pool().ctl + new Pool.Queue().top + slot)
      println(built())
    }
  }
}

object defshapes {
  def main(args: Array[String]): Unit = ds.Shapes.run()
}
