inline def foo(ys: Int*): Int = bar(ys*)
def bar(ys: Int*): Int = ys.sum

@main def Lw26arrayvarargs(): Unit =
  val xs: Array[Int] = Array(1, 2, 3)
  println(foo(xs*))
  val ys: Seq[Int] = Array(4, 5)
  println(foo(ys*))
  val ss: Array[String] = Array("a", "b")
  println(cat(ss*))

inline def cat(xs: String*): String = xs.mkString("-")
