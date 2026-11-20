package arrays

class Lib {
  def first[T](xs: Array[T]): T = xs(0)
  def refs[T <: AnyRef](xs: Array[T]): Int = xs.length
  def ints(xs: Array[Int]): Int = xs.length
}
