//> using scala 3.9.0
import java.util.function.{Consumer, DoubleConsumer}

trait Taker[T] { def take(c: Consumer[? >: T]): Boolean }
trait DoubleTaker extends Taker[java.lang.Double] {
  def take(c: DoubleConsumer): Boolean
}

class Builder(val n: Int) { def start(): Int = n }

class Runner private () {
  import Runner.BuilderOps
  def run(env: Array[String]): Int = new Builder(env.length).withEnv(env).start()
}

object Runner extends Runner() {
  private implicit class BuilderOps(val b: Builder) extends AnyVal {
    def withEnv(env: Array[String]): Builder = new Builder(b.n + env.length)
  }
}

object Jl30calls {
  def main(args: Array[String]): Unit = {
    val taker = new DoubleTaker {
      def take(c: DoubleConsumer): Boolean = { c.accept(2.5); true }
      def take(c: Consumer[? >: java.lang.Double]): Boolean = false
    }
    var seen = 0.0
    println(taker.take { (e: Double) => seen = e })
    println(seen)
    println(Runner.run(Array("a", "b")))
  }
}
