sealed abstract class DType
sealed class Float32 extends DType
sealed class Int32 extends DType
object Float32 extends Float32
object Int32 extends Int32

type ScalaType[U <: DType] <: Int | Float = U match
  case Float32 => Float
  case Int32 => Int

type Same[U <: DType] = U match
  case Float32 => Int
  case Int32 => Int

type Id[X] = X

abstract class Tensor[T <: DType]:
  def toArray: Array[ScalaType[T]]
  def same: Array[Same[T]]
  def ids[A](a: A): Array[Id[A]]

object IntTensor extends Tensor[Int32]:
  def toArray: Array[Int] = Array(1, 2, 3)
  def same: Array[Int] = Array(4, 5)
  def ids[A](a: A): Array[A] = Array[Any](a, a).asInstanceOf[Array[A]]

@main def matchtypearrays(): Unit =
  val t = IntTensor: Tensor[Int32]
  println(t.toArray.headOption)
  println(t.same.toList)
  println(t.ids("x").length)
