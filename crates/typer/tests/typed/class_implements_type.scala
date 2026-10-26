trait Scope:
  type Expr
  def expr(x: String): Expr
  def combine(e1: Expr, e2: Expr): Expr

object scope extends Scope:
  case class Expr(str: String)
  def expr(x: String) = Expr(x)
  def combine(e1: Expr, e2: Expr) = Expr(e1.str ++ e2.str)

object M:
  def f(s: Scope)(x: s.Expr) = s.combine(x, x)
  val e = f(scope)(scope.expr("a"))
