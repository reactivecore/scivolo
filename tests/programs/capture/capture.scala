//> using scala 3.9.0
object C:
  def times(n: Int, f: () => Unit): Unit =
    var i = 0
    while i < n do
      f()
      i += 1
  def count(): Int =
    var c = 0
    times(3, () => c += 1)
    c
  def names(): String =
    var s = ""
    times(2, () => s = s + "x")
    s
  def longs(): Long =
    var seed = 42L
    times(2, () => seed = seed * 3)
    seed
  def inClass(): Int =
    var k = 1
    new Object { k += 10 }
    k

@main def capture(): Unit =
  println(C.count())
  println(C.names())
  println(C.longs())
  println(C.inClass())
