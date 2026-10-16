//> using scala 3.9.0
// Lambdas of traits overriding a function's method: closures where the erasures agree, else
// anonymous classes with bridges.

trait ConTU[-T] extends (T => Unit):
  def apply(t: T): Unit

trait ConSR[+R] extends (String => R):
  def apply(t: String): R

trait ConII extends (Int => Int):
  def apply(t: Int): Int

object sambridges {
  def main(args: Array[String]): Unit = {
    val a: ConTU[Int] = (x: Int) => println(x)
    a(1)
    val b: ConSR[Int] = (x: String) => x.length
    println(b("abc"))
    val c: ConII = (x: Int) => x * 2
    println(c(21))
    val f: Int => Int = c
    println(f(4))
  }
}
