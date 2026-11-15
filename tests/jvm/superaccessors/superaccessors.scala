// Super accessors in the TASTy: a trait's super call of a member with a target name, and a
// protected member a class nested in a trait reaches, a super call of a method with implicit
// parameters, the generic signature of a class's accessor of a library trait's generic member.
import scala.annotation.targetName

package p {
  class A {
    protected def foo(): Int = 1
  }
}

class C1 { @targetName("f2") def f(): String = "C1" }
trait T1 extends C1 { @targetName("f2") override def f(): String = super.f() + " T1" }
class C2 extends C1 with T1 { @targetName("f2") override def f(): String = super.f() + " C2" }

trait Inner extends p.A {
  class InnerInner { def bar(): Int = foo() + 1 }
}
class D extends p.A with Inner

class C3 { def g[A](x: A)(implicit o: Ordering[A]): A = x }
trait T3 extends C3 { override def g[A](x: A)(implicit o: Ordering[A]): A = o.max(super.g(x), x) }
class C4 extends C3 with T3

trait T5[+A] extends scala.collection.IterableOps[A, List, List[A]] {
  def appended[B >: A](x: IterableOnce[B]): List[B] = super.concat(x)
}
abstract class C5[+A] extends T5[A]

object Superaccessors {
  def main(args: Array[String]): Unit = {
    println(new C2().f())
    val d = new D
    println(new d.InnerInner().bar())
    println(new C4().g(3))
    println(classOf[C5[?]].getMethods.filter(_.getName.contains("super")).map(_.toGenericString).mkString)
  }
}
