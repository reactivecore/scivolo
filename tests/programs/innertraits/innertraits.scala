//> using scala 3.9.0
class Box(val label: String):
  trait Shown:
    def show = s"$label: $name"
    def name: String
  class Item(val name: String) extends Shown
  object First extends Shown:
    def name = "first"
  private trait Quiet:
    def whisper = label.toLowerCase
  private class Muted extends Quiet
  def muted = new Muted().whisper
  trait Named extends Shown:
    def name = "named"
  class Tagged extends Named
  private trait Silent:
    def quiet = 0
  private class Hush extends Silent
  def hush = new Hush().quiet
  private trait Holder:
    class Inside extends Shown:
      def name = "inside"
    def make: Shown = new Inside
  private class HolderImpl extends Holder
  def inside = new HolderImpl().make.show
  private trait Loud extends Shown:
    def name = "loud"
  private class LoudImpl extends Loud
  def loud = new LoudImpl().show

object Boxes:
  val box = Box("Box")

class Outside extends Boxes.box.Shown:
  def name = "outside"

trait Shelf:
  val size = 3
  trait Slot:
    def describe = s"slot of $size"
  object Top extends Slot
  private object Hidden:
    println("hidden made")
  def hidden: AnyRef = Hidden

class Rack extends Shelf

trait Plain:
  val unit: Unit = ()
  lazy val lazyUnit: Unit = println("lazy unit")
  def run: Unit = unit

class Overriding extends Plain:
  override val unit: Unit = println("overriding unit")

object Patterns:
  lazy val _ = println("pattern forced")
  def touch(): Unit = println("touched")

@main def innertraits(): Unit =
  val box = Box("B")
  println(new box.Item("item").show)
  println(box.First.show)
  println(box.muted)
  println(new box.Tagged().show)
  println(box.hush)
  println(box.inside)
  println(box.loud)
  println(new Outside().show)
  val rack = new Rack
  println(rack.Top.describe)
  rack.hidden
  rack.hidden
  val o = new Overriding
  o.run
  o.lazyUnit
  o.lazyUnit
  Patterns.touch()
