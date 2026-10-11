//> using scala 3.9.0
object Init:
  println("init")
  val x: Int = 1

object Plain:
  val y: Int = 2

case class P(a: Int, b: Int)

@main def tuplespec(): Unit =
  val a = 3
  val b = 4
  val t = (a, b)
  println(t._1 + t._2)
  val any: Any = (1, 2)
  println(any.getClass.getName)
  println((1L, 2.0).getClass.getName)
  println(('c', true).getClass.getName)
  println(Tuple1(5).getClass.getName)
  println(("s", 1).getClass.getName)
  println((a + b, a).getClass.getName)
  println(new Tuple2(a, 1.5).getClass.getName)
  val u = (2L, 'x')
  println(u._2)
  t match
    case (x, y) => println(x * y)
  println(Init.x.getClass)
  println(Plain.y.getClass)
  println((5: Int).getClass)
  println(P(1, 2).a)
