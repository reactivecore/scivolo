import mixins.*

class Person(n: Int) extends Base(n) with Greeter {
  def name: String = "person" + n
  def label: String = "label " + twice
}

class Polite extends Overriding {
  def name: String = "polite"
  override def shout: String = super.shout + "!"
}

class Plain extends Named {
  def name: String = "plain"
}

trait Loud extends Greeter {
  override def shout: String = super.shout + "!!"
}

object Obj extends Loud {
  def name: String = "obj"
}

object Mixins {
  def main(args: Array[String]): Unit = {
    val p = new Person(3)
    println(p.greet("you"))
    println(p.shout)
    println(p.label)
    println(p.tick())
    val b: Base = p
    println(b.twice)
    val g: Greeter = new Polite
    println(g.greet("x"))
    println(g.shout)
    println(new Plain().name)
    println(Obj.shout)
  }
}
