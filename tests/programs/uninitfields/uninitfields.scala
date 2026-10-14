//> using scala 3.9.0
import scala.compiletime.uninitialized

class Meters(val n: Int) extends AnyVal

class Fields:
  var int: Int = uninitialized
  var long: Long = uninitialized
  var bool: Boolean = uninitialized
  var str: String = uninitialized
  var unit: Unit = uninitialized
  var meters: Meters = uninitialized
  private var hidden: Double = uninitialized
  var set = 7
  def bump(): Double =
    hidden += 1.5
    hidden

trait Slots:
  var slot: Int = uninitialized
  var label: String = uninitialized

class Holder extends Slots

object Registry:
  var name: String = uninitialized
  var count: Int = uninitialized

@main def uninitfields(): Unit =
  val f = Fields()
  println(f.int)
  println(f.long)
  println(f.bool)
  println(f.str)
  println(f.unit)
  println(f.meters.n)
  println(f.bump())
  println(f.set)
  f.int = 4
  f.str = "four"
  f.meters = Meters(9)
  println(f.int)
  println(f.str)
  println(f.meters.n)
  println(Registry.name)
  Registry.count += 2
  println(Registry.count)
  val h = Holder()
  println(h.slot)
  println(h.label)
  h.slot = 5
  println(h.slot)
