//> using scala 3.9.0
trait Counter:
  private val step = 2
  private val label = "n" + step
  private var count = 0
  private final val flag = true
  def tick(): String =
    count += step
    s"$label=$count $flag"

trait Other:
  private val label = "other".length
  private var count = 10
  println("Other")
  def bump(): Int =
    count += label
    count

class Both extends Counter, Other

object Outer:
  trait Inner:
    private val σ = "sigma"
    def show(): String = σ

object Solo extends Counter, Outer.Inner

trait Concrete:
  val v = 2

trait Abstract:
  val v: Int

class Both2 extends Concrete, Abstract

@main def privatetraits(): Unit =
  val b = Both()
  println(b.tick())
  println(b.tick())
  println(b.bump())
  println(Solo.tick())
  println(Solo.show())
  println(Both2().v)
