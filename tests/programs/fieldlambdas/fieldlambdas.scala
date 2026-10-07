//> using scala 3.9.0
object Ops:
  val base = 10
  val inc = (x: Int) => x + 1
  val greet: String => String = s => s"$s$base"
  val twice = (x: Int) => inc(x) * 2
  val sum = Option(6).map(x => x * base).getOrElse(0)
  println(s"Ops ready: $sum")

class Adder(p: Int):
  val add = (x: Int) => x + p
  val scaled = Option(2).map(_ * p)

class Counter(val start: Int, step: Int):
  val next = (n: Int) => start + n * step
  def show = s"Counter($start)"

class Boxed(p: Int):
  val get = { val f = (x: Int) => x * p; f }

class Tagged(tag: String):
  println(s"tag $tag")
  def show = tag

class Local(p: Int):
  val sum =
    def plus(x: Int) = x + p
    plus(1)

@main def fieldlambdas(): Unit =
  println(Ops.inc(2))
  println(Ops.greet("a"))
  println(Ops.twice(3))
  val a = Adder(5)
  println(a.add(1))
  println(a.scaled)
  val c = Counter(3, 4)
  println(c.next(2))
  println(c.show)
  println(Boxed(7).get(2))
  println(Tagged("t").show)
  println(Local(4).sum)
