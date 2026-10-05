//> using scala 3.9.0
object A:
  def sum(a: Array[Int]): Int =
    var s = 0
    var i = 0
    while i < a.length do
      s += a(i)
      i += 1
    s
  def fill(n: Int): Array[Int] =
    val a = new Array[Int](n)
    var i = 0
    while i < n do
      a.update(i, i * i)
      i += 1
    a
  def first(a: Array[String]): String = a(0)
  def strings(n: Int): Array[String] =
    val a = new Array[String](n)
    a.update(0, "x")
    a
  def doubles(): Double =
    val d = new Array[Double](2)
    d.update(1, 2.5)
    d(0) + d(1)

@main def arrays(): Unit =
  println(A.sum(A.fill(5)))
  println(A.first(A.strings(3)))
  println(A.doubles())
