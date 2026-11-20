//> using dep org.scala-lang:scala-reflect:2.13.16
package macros

import scala.language.experimental.macros
import scala.reflect.macros.blackbox.Context

object M {
  def one: Int = macro impl
  def impl(c: Context): c.Expr[Int] = c.universe.reify(1)
}
