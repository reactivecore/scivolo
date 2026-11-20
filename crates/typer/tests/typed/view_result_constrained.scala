import scala.language.implicitConversions

class Op[T]
trait Skin[C <: Skinnable]
trait Skinnable {
  def skinProperty: Op[Skin[?]]
}
class Property[T](val deleg: Op[T])
object Property {
  implicit def toProperty[T](p: Op[T]): Property[T] = new Property[T](p)
}

trait ViewResultConstrained {
  def deleg: Skinnable
  def inferred = deleg.skinProperty
  inferred: Property[Skin[? <: Skinnable]]
}
