import scala.language.implicitConversions

class HasId:
  def id: String = "conv"

object ExtensionA:
  extension (self: Any) def id: String = "A"

object ExtensionB:
  extension (self: Any) def id: String = "B"

object Conv:
  given Conversion[Int, HasId] = _ => HasId()

object Main:
  def main(args: Array[String]): Unit =
    import Conv.given
    import ExtensionA.*
    import ExtensionB.*
    println(1.id)
