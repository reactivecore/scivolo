//> using scala 3.9.0
// Lambdas of single abstract method types that are not functional interfaces, expanded to
// anonymous classes: abstract classes, traits with a class parent or with fields.

abstract class Inc {
  def apply(x: Int): Int
  def twice(x: Int): Int = apply(apply(x))
}

abstract class Mapper[A, B] {
  def map(a: A): B
}

class Base {
  def name: String = "base"
}

trait Named extends Base {
  def label(n: Int): String
}

trait WithField {
  lazy val base: Int = 100
  def run(x: Int): Int
}

case class Text(s: String)

class Holder(val n: Int) {
  val nested: Mapper[Int, String] = x => {
    val f: Mapper[String, String] = s => s + n
    f.map(x.toString)
  }
  def adder(k: Int): Mapper[Int, String] = x => {
    val g: Inc = y => y + k
    g(x).toString
  }
}

object Sams {
  val inc: Inc = x => x + 1
  val len: Mapper[String, Int] = s => s.length
  val named: Named = n => "label " + n
  val wf: WithField = x => x * 2
  given Conversion[String, Text] = Text(_)
}

object Test {
  import Sams.given
  def run(): Unit = {
    println(Sams.inc.twice(1))
    println(Sams.len.map("abc"))
    println(Sams.named.label(2) + Sams.named.name)
    println(Sams.wf.run(3) + Sams.wf.base)
    val t: Text = "hi"
    println(t)
    val k = 10
    val add: Inc = x => x + k
    println(add(5))
    val h = new Holder(4)
    println(h.nested.map(2))
    println(h.adder(1).map(2))
  }
}

@main def samclasses(): Unit = Test.run()
