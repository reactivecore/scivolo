import scala.reflect.ClassTag

sealed trait Base
case object A extends Base
case object B extends Base
class Outer { class In }
class V(val x: Int) extends AnyVal

object Tags {
  object Foo
  val foo = "foo"
  def tag[T](using t: ClassTag[T]): ClassTag[T] = t
  def arr[T: ClassTag](x: T): Array[Array[T]] = Array(Array(x))
  def t1 = tag[Array[Int]]
  def t2 = tag[Option[Int]]
  def t3 = tag[(String, Int)]
  def t4 = tag[A.type]
  def t5 = tag[Array[A.type]]
  def t6 = tag[Array[Array[String]]]
  def t7 = tag[String | Int]
  def t8 = tag[Base & Product]
  def t9 = tag[V]
  def t10 = tag[Outer#In]
  def t11 = tag[AnyRef]
  def t12 = tag[Matchable]
  def t13 = tag[Unit]
  def t14 = tag[Int | 1]
  def t15 = tag[Int | Nothing]
  val same = reflect.classTag[Int | Int]
  def a1 = Array(A, B)
  def a2 = Array(Array(1), List(1))
  def a3 = arr("x")
  def v1 = implicitly[ValueOf[1]].value
  def v2 = implicitly[ValueOf["s"]].value
  def v3 = implicitly[ValueOf[Unit]].value
  def v4 = implicitly[ValueOf[Foo.type]].value
  def v5 = implicitly[ValueOf[foo.type]].value
}
