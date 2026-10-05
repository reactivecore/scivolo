trait ToInt[-T] extends (T => Int):
  def apply(t: T): Int

trait Unit0[+R] extends (() => R):
  def apply(): R

trait Cmp:
  def cmp(a: Int, b: Int): Boolean

class C1

object Main:
  def run(): Int =
    val length: ToInt[String] = (s: String) => s.length
    val inferred: ToInt[String] = s => s.length
    val seven: Unit0[Int] = () => 7
    val r: Runnable = () => println("x")
    val c: Cmp = (a, b) => a < b
    val t = new Thread(() => println("t"))
    val k = classOf[C1]
    val n = classOf[Array[Int]]
    val a = try 1 catch case _: Exception => 2
    val b =
      try
        println("x")
        3
      catch
        case e: RuntimeException => 4
        case _: Throwable => 5
      finally println("done")
    println(Array("a").toList)
    length("x") + seven() + inferred("y") + a + b
