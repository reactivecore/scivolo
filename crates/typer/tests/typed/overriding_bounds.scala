abstract class Tp
trait Lambda extends Tp {
  type ThisName <: CharSequence
  def paramNames: List[ThisName]
}
abstract class BoundType extends Tp {
  type BT <: Tp
  val binder: BT
}
abstract class ParamRef extends BoundType {
  type BT <: Lambda
  def paramNum: Int
  def paramName: binder.ThisName = binder.paramNames(paramNum)
  def names = binder.paramNames
  def length = paramName.length
}
