//> using scala 3.9.0
object Narrow:
  val b: Byte = 100
  val s: Short = -300
  final val fb: Byte = 7
  def neg: Byte = -128
  def big: Short = 32767
  def char: Char = 65
  def sum: Int = fb + fb
  def float: Float = 16777217
  def pair: (Byte, Short) = (1, 2)
  def byteList: List[Byte] = List(1, 2, 3)
  def side: Byte = { println("side"); 5 }
  def kind(x: Byte): String = x match
    case 1 => "one"
    case 2 => "two"
    case _ => "many"
  def short(x: Short): String = x match
    case 300 => "three hundred"
    case _ => "other"

@main def bytes_shorts(): Unit =
  println(Narrow.b)
  println(Narrow.s)
  println(Narrow.fb)
  println(Narrow.neg)
  println(Narrow.big)
  println(Narrow.char)
  println(Narrow.sum)
  println(Narrow.float)
  println(Narrow.pair)
  println(Narrow.byteList.sum)
  println(Narrow.side)
  println(Narrow.kind(1))
  println(Narrow.kind(9))
  println(Narrow.short(300))
  println(Narrow.short(3))
