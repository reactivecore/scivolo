// Macro implementations whose quotes have holes without captured references, compiled but not
// run: their NIR is compared with Scala Native's by hand (`scripts/nir_compare.py`).
package pk

import java.util.concurrent.ConcurrentHashMap
import scala.quoted.*

class Name(val value: String)
class File(val value: String)
class Line(val value: Int)

object Macros {
  def owner(using Quotes)(s: quotes.reflect.Symbol): quotes.reflect.Symbol = {
    var o = s
    while (o.flags.is(quotes.reflect.Flags.Synthetic)) o = o.owner
    o
  }

  def nameImpl(using Quotes): Expr[Name] = {
    val name = owner(quotes.reflect.Symbol.spliceOwner).name.stripSuffix("$")
    '{new Name(${Expr(name)})}
  }

  private val files = new ConcurrentHashMap[Any, Option[String]]()

  def fileImpl(using Quotes): Expr[File] = {
    val source = quotes.reflect.Position.ofMacroExpansion.sourceFile
    val file = files.computeIfAbsent(source, _ => None).getOrElse(source.path)
    '{new File(${Expr(file)})}
  }

  def lineImpl(using Quotes): Expr[Line] = {
    val line = quotes.reflect.Position.ofMacroExpansion.startLine + 1
    '{new Line(${Expr(line)})}
  }

  def bounded(size: Expr[Int])(using Quotes): Expr[Int] =
    '{ if ($size < 0) throw new IllegalArgumentException("negative size") else $size }

  def twice(x: Expr[Int])(using Quotes): Expr[Int] = '{ Math.addExact($x, $x) }

  type Str = String

  def listed(x: Expr[Int])(using Quotes): Expr[Int] = '{ val xs: List[Int] = List($x); xs.length }

  def described(x: Expr[Any])(using Quotes): Expr[String] =
    '{ $x match { case s: Str => s; case _ => "other" } }
}

object quotesources {
  def main(args: Array[String]): Unit = println("quotes compiled")
}
