// Members selected on a value whose type gives a class type parameter a wildcard argument.
import java.util.function.{Consumer, Function}

class Box[T](val value: T):
  def applyNow[V](f: Function[? >: T, ? <: V]): V = f.apply(value)
  def feed(c: Consumer[? >: T]): Unit = c.accept(value)

def run[T, V](b: Box[T], f: Function[? >: T, ? <: V], c: Consumer[? >: T]): V =
  var z: Box[? <: T] = null
  z = b
  z.feed(c)
  z.applyNow[V](f)

def first(g: Function[Int, ? <: Seq[String]]): String =
  var h = g
  h.apply(3).head

@main def wildprefix(): Unit =
  val f: Function[Any, String] = x => "got " + x
  val c: Consumer[Any] = x => println("fed " + x)
  println(run(Box(42), f, c))
  println(first(n => Seq.fill(n)("s" + n)))
