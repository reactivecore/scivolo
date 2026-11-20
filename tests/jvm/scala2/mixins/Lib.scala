package mixins

trait Named {
  def name: String
}

trait Greeter extends Named {
  def greet(other: String): String = "hello " + other + " from " + name
  def shout: String = greet("all").toUpperCase
}

abstract class Base(val n: Int) {
  def twice: Int = n * 2
  def label: String
  def tick(): Int = n + 1
}

trait Counter {
  var count: Int = 0
  def bump(): Int = { count += 1; count }
}

trait Valued {
  val start: Int = 10
}

trait Lazy {
  lazy val cached: String = "cached"
}

trait Overriding extends Greeter {
  override def greet(other: String): String = "hi " + other
}
