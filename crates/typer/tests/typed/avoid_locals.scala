class A(val x: Int):
  class B extends A(2)

trait Key { type Value }
trait F[-T] { def f(t: T): Unit }

object Test:
  val key = new Key { type Value = Some[Int] }
  val v: key.Value = Some(1)
  def inner = { val a = A(1); a.B() }
  def list = { class L(val n: Int); List(new L(1)) }
  def fun = { object X; () => X }
  def contra = { class C extends Key; new F[C] { def f(t: C) = () } }
  def pair = { val k = new Key { type Value = Int }; (k, 1) }
