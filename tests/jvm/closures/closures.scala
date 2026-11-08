class Counter(start: Int) {
  var n: Int = start
  def adder(): Int => Int = x => x + n
  def bump(): () => Unit = () => n += 1
}

trait Greeter {
  def name: String
  def greet(): () => String = () => "hi " + name
  def shout(): String => String = s => s + "!"
}

class G extends Greeter { def name: String = "g" }

trait IntOp { def op(x: Int): Int }

object Closures {
  def useOp(o: IntOp): Int = o.op(4)

  def twice(f: Int => Int, x: Int): Int = f(f(x))
  def apply2(f: (String, Int) => String): String = f("a", 3)
  def run(r: Runnable): Unit = r.run()
  def compose(f: Double => Double, g: Double => Double): Double => Double = x => g(f(x))

  def main(args: Array[String]): Unit = {
    println(twice(x => x * 3, 2))
    val k = 10
    println(twice(x => x + k, 1))
    println(apply2((s, n) => s * n))
    val c = new Counter(5)
    val f = c.adder()
    println(f(1))
    c.bump()()
    println(f(1))
    run(() => println("run"))
    val sq: Double => Double = d => d * d
    println(compose(sq, d => d + 0.5)(3.0))
    val ss: String => Int = s => s.length
    println(ss("four"))
    val b: Long => Boolean = l => l > 2L
    println(b(3L))
    val u: () => String = () => "unit"
    println(u())
    val g = new G
    println(g.greet()())
    println(g.shout()("hey"))
    println(useOp(x => x * k))
    var count = 0
    (1 to 3).foreach(i => count += i)
    println(count)
  }
}
