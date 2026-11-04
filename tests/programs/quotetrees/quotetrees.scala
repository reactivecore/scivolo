import scala.quoted.*

package object quotetreespkg { val v: Int = 1; var w: Int = 2 }

object quotetrees {
  def main(args: Array[String]): Unit = println("lowered")

  def defaults(using Quotes): Expr[Int] = '{ Str("a") }
  def opAssign(using Quotes): Expr[Int] = '{ var x = 1; x += 1; x }
  def packageObject(using Quotes): Expr[Int] = '{ quotetreespkg.v + 1 }
  def packageOpAssign(using Quotes): Expr[Int] = '{ quotetreespkg.w += 1; quotetreespkg.w }
  def updateOpAssign(using Quotes): Expr[Int] = '{ val a = Array(1); a(0) += 2; a(0) }
  def liftedDefaults(using Quotes): Expr[Int] = '{ new Box().m(2) }

  object Str {
    def apply(s: String, n: Int = 2): Int = n
  }
  class Box { def m(i: Int = 1, j: Int = 2): Int = i + j }
}
