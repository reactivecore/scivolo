trait Foo {
  type A[T]
  var arr: Array[A[Int]] = null
}

class Bar extends Foo {
  type A[T] = Int
}

trait Foo2 {
  type Dummy
  type A[T] <: Dummy
  var arr: Array[A[Int]] = null
}

class Bar2 extends Foo2 {
  type Dummy = Any
  type A[T] = Int
}

trait Foo3 {
  type A[T] <: Object
  var arr: Array[A[String]] = null
}

class Bar3 extends Foo3 {
  type A[T] = String
}

trait Foo4 {
  type A[T]
  type S
  type E <: A[Int]
  type F <: S
}

class Bar4 extends Foo4 {
  type A[T] = Int
  type S = Long
  def e(xs: Array[E]): Array[E] = xs
  def f(xs: Array[F]): Array[F] = xs
  def m[U <: E](xs: Array[U]): Array[U] = xs
}

class Out4(val bar: Bar4) {
  def e(xs: Array[bar.E]): Array[bar.E] = xs
}

class Ref5[R <: Foo { type A[T] = Long }](val f: R) {
  def arr(xs: Array[f.A[Int]]): Array[f.A[Int]] = xs
}

class Bar5 extends Foo {
  type A[T] = Long
}

def erased(c: Class[?], name: String, param: Class[?]): String =
  c.getMethod(name, param).getReturnType.getSimpleName

object Hkmemberarrays {
  def main(args: Array[String]): Unit = {
    val bar = new Bar
    bar.arr = Array.ofDim[Int](1)
    bar.arr(0) = 123
    println(bar.arr(0))

    val bar2 = new Bar2
    bar2.arr = Array.ofDim[Int](1)
    bar2.arr(0) = 124
    println(bar2.arr(0))

    val bar3 = new Bar3
    bar3.arr = Array.ofDim[String](1)
    bar3.arr(0) = "125"
    println(bar3.arr(0))

    println(erased(classOf[Bar4], "e", classOf[Array[Int]]))
    println(erased(classOf[Bar4], "f", classOf[Array[Long]]))
    println(erased(classOf[Bar4], "m", classOf[Array[Int]]))
    println(erased(classOf[Out4], "e", classOf[Array[Int]]))

    val ref5 = new Ref5[Bar5](new Bar5)
    println(ref5.arr(Array(126L))(0))
    println(erased(classOf[Ref5[?]], "arr", classOf[Array[Long]]))
  }
}
