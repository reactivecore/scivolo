object Use {
  def f(r: Types.ParamRef) = r.paramName.length
}

object Types {
  abstract class Tp
  trait Lambda extends Tp { self =>
    type ThisName <: CharSequence
    def paramNames: List[ThisName]
  }
  abstract class BoundType extends Tp {
    type BT <: Tp
    val binder: BT
  }
  abstract class ParamRef extends BoundType {
    def paramName: binder.ThisName = binder.paramNames(paramNum)
    type BT <: Lambda
    def paramNum: Int
  }
}
