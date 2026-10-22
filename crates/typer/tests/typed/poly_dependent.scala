object Main {
  trait Foo { type X }
  class Bar extends Foo { type X = Int }
  val g = [A <: Foo] => (f: A) => (x: f.X) => x
  def main(args: Array[String]): Unit = {
    val b = new Bar
    println(g(b)(1))
  }
}
