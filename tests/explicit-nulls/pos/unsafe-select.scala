//> using options -Yexplicit-nulls --fresh -classpath libs/javalib-tasty-0.5.12.jar
import scala.language.unsafeNulls

object UnsafeSelect:
  def isNull(x: String | Null): Boolean = x eq null
  def notNull(x: String | Null): Boolean = x ne null
  def len(x: String | Null): Int = x.length
  def trimmed(x: String | Null): String = x.trim.nn
  def leftNull(x: Null | String): Boolean = x eq null
  def twice(x: String | Null | Null): Int = x.length
  def same(x: String | Null, y: AnyRef): Boolean = x eq y
  def java(x: String): Boolean = x.substring(1) eq null
  def javaLen(x: String): Int = x.substring(1).length
  def nullFirst(x: String | Null): Boolean = null eq x
  def any(x: Int | Null): Boolean = x == null
  def head(xs: List[Int] | Null): Int = xs.head
  def hash[T <: AnyRef](x: T | Null): Int = x.hashCode
  def out: Boolean = System.out eq null
  def prop: Int = System.getProperty("a").length
  val fromJava: String = System.getProperty("b")
