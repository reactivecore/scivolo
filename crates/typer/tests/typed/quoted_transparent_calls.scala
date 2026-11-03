import scala.quoted.*
import scala.compiletime.*

object Test:
  def implC(using Quotes)(tpe: quotes.reflect.TypeRepr): Expr[AnyRef] =
    tpe.asType match
      case '[t] => '{ summonInline[t].asInstanceOf[AnyRef] }

  def implD[T: Type](using Quotes): Expr[AnyRef] =
    '{ summonInline[T].asInstanceOf[AnyRef] }
