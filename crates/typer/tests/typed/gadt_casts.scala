enum Expr[T]:
  case IntLit(i: Int) extends Expr[Int]
  case Pair[A, B](a: Expr[A], b: Expr[B]) extends Expr[(A, B)]

enum Eql[A, B]:
  case Refl[X]() extends Eql[X, X]

object GadtCasts:
  def eval[T](e: Expr[T]): T = e match
    case Expr.IntLit(i) => i
    case Expr.Pair(a, b) => (eval(a), eval(b))

  def coerce[A, B](eq: Eql[A, B], as: List[A]): List[B] = eq match
    case Eql.Refl() => as

  def inferred[T](e: Expr[T], t: T): List[T] = e match
    case Expr.IntLit(_) => List(t)
    case _ => Nil

  def ahead[T](e: Expr[T], t: T): Int =
    def m: Int = e match
      case Expr.IntLit(i) => k + i
      case _ => 0
    def k = 1
    m
