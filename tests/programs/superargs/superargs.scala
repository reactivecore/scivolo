//> using scala 3.9.0
class Base(val n: Int, val tag: String):
  def show = s"$tag($n)"

class Doubled(m: Int) extends Base(m * 2, "doubled"):
  def original = m

class Measured(k: Int, label: String) extends Base(k + label.length, label)

class Chained(x: Int) extends Doubled(x + 1)

class Named(val name: String) extends Base(name.length, "named " + name)

@main def superargs(): Unit =
  println(Doubled(3).show)
  println(Doubled(4).original)
  println(Measured(1, "abc").show)
  println(Chained(2).show)
  println(Chained(2).original)
  val named = Named("ada")
  println(s"${named.name} ${named.show}")
