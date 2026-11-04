class Box:
  opaque type Str = String
  object Str:
    def apply(s: String): Str = s
  inline def lower(s: Str): String = s.toLowerCase
  extension (s: Str) transparent inline def upper: String = s.toUpperCase

object Time:
  opaque type Secs = Int
  def secs(n: Int): Secs = n
  extension (a: Secs)
    inline def plus(b: Secs): Secs = secs(a + b)
    def value: Int = a
  transparent inline def zero: Secs = 0

object opaqueinlines:
  def main(args: Array[String]): Unit =
    val box = new Box()
    import box.*
    val a: Str = Str("aSd")
    println(box.lower(a))
    println(a.upper)
    val t = Time.secs(3).plus(Time.secs(4))
    println(t.value)
    val z: Time.Secs = Time.zero
    println(z.value)
