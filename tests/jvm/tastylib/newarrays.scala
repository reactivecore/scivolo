import scala.reflect.ClassTag

class NewArrays[T: ClassTag]:
  def generic(n: Int) = new Array[T](n)
  def ints(n: Int) = new Array[Int](n)
  def strings(n: Int) = new Array[String](n)
  def nested(n: Int) = new Array[Array[Int]](n)
