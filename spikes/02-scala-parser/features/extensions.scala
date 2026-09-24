extension (s: String)
  def shout: String = s.toUpperCase + "!"
  def whisper: String = s.toLowerCase

extension [T](xs: List[T])(using ord: Ordering[T])
  def smallest: T = xs.min
  infix def minus(ys: List[T]): List[T] = xs.diff(ys)

extension (i: Int) def squared: Int = i * i

object Ops:
  extension (x: Double)
    def +++(y: Double): Double = x + y
  end extension

val a = "hi".shout
val b = List(3, 1, 2).smallest
val c = 4.squared
