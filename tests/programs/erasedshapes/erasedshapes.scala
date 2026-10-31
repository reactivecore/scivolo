//> using scala 3.9.0
trait Wrapped[E]:
  def inner: java.util.Collection[E]
  def toArray[T <: AnyRef](a: Array[T]): Array[T] = inner.toArray[T](a)

implicit class ToIt[A](private val self: java.lang.Iterable[A]) extends AnyVal:
  def ops: ItOps[A] = new ItOps[A](self)

class ItOps[A](val self: java.lang.Iterable[A]) extends AnyVal:
  def first: A = self.iterator().next()
  def again: ItOps[A] = new ItOps[A](self)

object Shapes:
  def firstOf(l: java.util.ArrayList[String]): String =
    val o = l.ops
    o.first

  def hex(i: Int): String =
    @inline def padded(x: Int): String = Integer.toHexString(x)
    padded(i) + padded(i + 1)

  def wrap[E](c: java.util.Collection[E]): Wrapped[E] = new Wrapped[E] { def inner = c }

  def copied(c: java.util.Collection[String]): Array[String] =
    val a = c.toArray(new Array[String](0))
    a

  def lub(c: Boolean, x: Int, y: java.lang.Long): Int =
    if c then x else y
    1

  def captured(a: Array[Byte]): Int =
    var last: Array[Byte] = null
    val f = () => last = a
    f()
    if a eq last then last.length else -1

  def ofDim(n: Int): Int = Array.ofDim[Any](n).length

  def ofDims(n: Int): Int = Array.ofDim[String | Null](n).length

  def again(l: java.util.ArrayList[String]): String = new ItOps(l).again.first + l.ops.again.first

  def init(parts: Array[String]): String = parts.init.mkString("/")

@main def erasedshapes(): Unit =
  println(Shapes.hex(10))
  val list = new java.util.ArrayList[String]()
  list.add("a")
  println(Shapes.wrap(list).toArray(new Array[String](1)).length)
  println(Shapes.copied(list).length)
  println(Shapes.lub(true, 1, 2L))
  println(Shapes.captured(new Array[Byte](3)))
  println(Shapes.ofDim(4))
  println(Shapes.init(Array("x", "y", "z")))
  println(Shapes.firstOf(list))
  println(Shapes.ofDims(2))
  println(Shapes.again(list))
