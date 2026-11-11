import scala.quoted.*

object QuotePatternVarargs {
  def parts(x: Expr[StringContext])(using Quotes): Option[Seq[Expr[String]]] = x match {
    case '{ StringContext(${Varargs(args)}*) } => Some(args)
    case _ => None
  }
}
