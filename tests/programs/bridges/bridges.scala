//> using scala 3.9.0

/** SAM lambdas called through `Function1`/`Function0`, and `getInterfaces` of classes and arrays;
  * `bridges.check` is written by hand.
  */
trait ToInt[-T] extends (T => Int):
  def apply(t: T): Int

trait ToStr[-T] extends (T => String):
  def apply(t: T): String

trait FromStr[+R] extends (String => R):
  def apply(t: String): R

trait Unit0[+R] extends (() => R):
  def apply(): R

trait T1
trait T2 extends T1
trait T3 extends T2
class C1 extends T2
class C2 extends C1 with T3 with T1

def viaFunction[A, B](f: A => B, a: A): B = f(a)

@main def bridges(): Unit =
  val length: ToInt[String] = (s: String) => s.length
  println(viaFunction(length, "four"))
  val plusOne: ToInt[Int] = (i: Int) => i + 1
  println(viaFunction(plusOne, 41))
  val show: ToStr[Char] = (c: Char) => s"<$c>"
  println(viaFunction(show, 'x'))
  val parse: FromStr[Int] = (s: String) => s.toInt
  println(viaFunction(parse, "7") + 1)
  val seven: Unit0[Int] = () => 7
  val f: () => Int = seven
  println(f())
  println(classOf[C1].getInterfaces.toList)
  println(classOf[C2].getInterfaces.toList)
  println(classOf[Array[Int]].getInterfaces.toList)
