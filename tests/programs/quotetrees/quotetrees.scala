import scala.quoted.*

package object quotetreespkg { val v: Int = 1 }

object quotetrees {
  def main(args: Array[String]): Unit = println("lowered")

  def defaults(using Quotes): Expr[Int] = '{ Str("a") }
  def opAssign(using Quotes): Expr[Int] = '{ var x = 1; x += 1; x }
  def packageObject(using Quotes): Expr[Int] = '{ quotetreespkg.v + 1 }

  object Str {
    def apply(s: String, n: Int = 2): Int = n
  }
}
