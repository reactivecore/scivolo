//> using scala 3.9.0

class Box(val n: Int):
  override def toString = s"Box($n)"

def pick(i: Int) = if i > 0 then Some(i) else None

def mixed(i: Int) = if i > 0 then i else "none"

def describe(x: Int | String) = x match
  case i: Int => s"int $i"
  case s: String => s"string $s"

@main def unions =
  println(pick(1))
  println(pick(0))
  println(describe(mixed(2)))
  println(describe(mixed(0)))
  val parsed = try "12".toInt catch case _: NumberFormatException => "bad"
  println(parsed)
  val boxed = try (java.lang.Short.valueOf(100.toShort): Any).asInstanceOf[Int] catch case _: Throwable => "cce"
  println(boxed)
  val all = List(1, "a", 2.5)
  println(all.map(_.toString).mkString(","))
  new Box(3)
