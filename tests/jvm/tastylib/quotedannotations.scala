import scala.quoted.*

def tags(l: Expr[annotlib.Lib])(using Quotes): Expr[Int] = '{ val t = $l.tags; t.size }
def marks(l: Expr[annotlib.Lib])(using Quotes): Expr[Int] = '{ val m = $l.marks; m.size }
