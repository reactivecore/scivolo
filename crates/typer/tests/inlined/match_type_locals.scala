import scala.compiletime.constValue
import scala.compiletime.ops.int.S

object M:
  type Len[X <: Tuple] <: Int = X match
    case EmptyTuple => 0
    case x *: xs => S[Len[xs]]

  inline def twice[X <: Tuple](x: X): Int =
    val n = constValue[Len[X]]
    val m: Len[X] = constValue[Len[X]]
    (n: Len[X]) + m

  type Same[X <: Tuple] <: Tuple = X match
    case EmptyTuple => EmptyTuple
    case x *: xs => x *: Same[xs]

  inline def same[X <: Tuple](x: X): Same[X] = x.asInstanceOf[Same[X]]

  inline def cat[X <: Tuple, Y <: Tuple](x: X, y: Y): Tuple = (x ++ same[Y](y)).asInstanceOf[Tuple]

object Test:
  val a = M.twice((1, "b"))
  val b = M.cat((1, "b"), (2.0, 'c'))
