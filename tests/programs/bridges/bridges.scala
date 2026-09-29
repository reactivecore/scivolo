//> using scala 3.9.0

/** SAM lambdas whose classes lack the generic bridge method, and `getInterfaces` as the JVM
  * lists them; `bridges.check` is written by hand.
  */
trait ToInt[-T] extends (T => Int):
  def apply(t: T): Int

trait FromInt[+R] extends (Int => R):
  def apply(t: Int): R

trait Unit0 extends (() => Unit):
  def apply(): Unit

trait T1
trait T2 extends T1
trait T3 extends T2
class C1 extends T2
class C2 extends C1 with T3 with T1

def viaFunction[A, B](f: A => B, a: A): B = f(a)

@main def bridges(): Unit =
  val length: ToInt[String] = (s: String) => s.length
  println(viaFunction(length, "four"))
  val twice: FromInt[Int] = (i: Int) => i * 2
  println(viaFunction(twice, 21))
  val hello: Unit0 = () => println("unit")
  val f: () => Unit = hello
  f()
  println(classOf[C1].getInterfaces.toList)
  println(classOf[C2].getInterfaces.toList)
