//> using scala 3.9.0
object M:
  def small(n: Int): String = n match
    case 1 => "one"
    case 2 => "two"
    case _ => "many"
  def switch(n: Int): String = n match
    case 1 => "one"
    case 2 | 3 => "few"
    case 4 => "four"
    case 5 => "five"
    case k => "n" + k
  def strings(s: String): Int = s match
    case "a" => 1
    case "b" => 2
    case "c" | "d" => 3
    case "e" => 5
    case _ => 0
  def guards(n: Int): String = n match
    case k if k < 0 => "neg"
    case 0 => "zero"
    case k if k > 100 =>
      val big = k / 100
      "big" + big
    case _ => "pos"
  def chars(c: Char): Int = c match
    case 'a' => 1
    case 'b' => 2
    case 'c' => 3
    case 'd' => 4
    case _ => 0
  def few(c: Char): Int = c match
    case 'x' | 'y' => 1
    case _ => 0
  def nomatch(n: Long): Int = n match
    case 1L => 1
  def unit(n: Int): Unit = n match
    case 1 => println("one")
    case _ =>
  def nested(a: Int, b: Int): String = a match
    case 0 => b match
      case 0 => "both"
      case _ => "a"
    case _ => "none"
  def ap(f: Int => String, x: Int): String = f(x)
  def inLambda(n: Int): String = ap(x => x match
    case 1 => "l1"
    case _ => "l" + x, n)
  def statement(n: Int): Int =
    var count = 0
    n match
      case 1 => count += 1
      case 2 => count += 2
      case _ => count -= 1
    count

@main def matches(): Unit =
  println(M.small(2))
  println(M.switch(3))
  println(M.switch(9))
  println(M.strings("d"))
  println(M.strings("z"))
  println(M.guards(-4))
  println(M.guards(250))
  println(M.guards(7))
  println(M.chars('c'))
  println(M.few('y'))
  println(M.nomatch(1L))
  M.unit(1)
  println(M.nested(0, 0))
  println(M.nested(0, 1))
  println(M.inLambda(1))
  println(M.inLambda(5))
  println(M.statement(2))
  println(M.statement(7))
