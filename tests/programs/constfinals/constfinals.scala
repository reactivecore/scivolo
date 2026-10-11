//> using scala 3.9.0
object Consts:
  val x: 1 = 1
  final val y = x
  val z: 1 = y
  final val a = { println("a"); 2 }
  val b: 2 = a
  final def g: 4 = { println("g"); 4 }
  final val gv1: 4 = g
  final val gv2: 4 = g
  val str: "s" = "s"
  final val str2 = str

@main def constfinals(): Unit =
  println(Consts.z + Consts.b + Consts.gv1 + Consts.gv2)
  println(Consts.str2)
