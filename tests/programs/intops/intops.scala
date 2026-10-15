//> using scala 3.9.0

def calc(n: Int): Int =
  val neg = -7 * n
  val b: Byte = (-5).toByte
  val s: Short = (-300).toShort
  val min = Int.MinValue + n - n
  val q = min / -1
  val r = neg % -1
  val r2 = n % -3
  q / 1000000 + r + r2 + b + s + neg / -2

@main def intops(args: String*): Unit =
  println(calc(args.headOption.fold(20)(_.toInt)))
