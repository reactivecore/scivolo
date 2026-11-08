trait UniqueImpl {
  self: selfnames.type =>
  def pair = (1, 2)
  def handler: PartialFunction[Throwable, Option[Int]] = { case _: Exception => None }

  abstract class Compound {
    def run(): Int
    lazy val (first, (second, third: (() => Int)), fourth) = {
      val q = new java.util.concurrent.LinkedBlockingQueue[Int](1)
      q.put(3)
      (1, (2, () => q.take()), () => run())
    }
    def attempt = try Some(first) catch handler
  }

  class Dummy {
    private val (x, y) = pair
    def sum = x + y
  }
}

final class Zipped(private val colls: (List[Int], List[String])) extends AnyVal {
  def iterator: Iterator[(Int, String)] = colls._1.iterator.zip(colls._2.iterator).map { case (a, b) => (a, b) }
}

object Zipped {
  final class Ops(private val x: (Int, Int)) extends AnyVal {
    def swapped: (Int, Int) = x match { case (a, b) => (b, a) }
  }
}

object selfnames extends UniqueImpl {
  def main(args: Array[String]): Unit = {
    val c = new Compound { def run() = 4 }
    println(c.attempt)
    println(c.second + c.third() + c.fourth())
    println(new Dummy().sum)
    println(new Zipped((List(1, 2), List("a", "b"))).iterator.toList)
    println(new Zipped.Ops((1, 2)).swapped)
  }
}
