import java.lang.invoke.{MethodHandles, MethodType, WrongMethodTypeException}
import scala.runtime.IntRef

class Foo:
  def neg(x: Int): Int = -x
  def greet(s: String): Unit = println(s"hello $s")
  def tag(s: String): String = s"<$s>"

object Sigpoly:
  def main(args: Array[String]): Unit =
    val l = MethodHandles.lookup()
    val neg = l.findVirtual(classOf[Foo], "neg", MethodType.methodType(classOf[Int], classOf[Int]))
    val greet = l.findVirtual(classOf[Foo], "greet", MethodType.methodType(classOf[Unit], classOf[String]))
    val foo = new Foo
    println(neg.invokeExact(foo, 42): Int)
    greet.invokeExact(foo, "handle"): Unit
    try
      val any = neg.invokeExact(foo, 1)
      println(s"unexpected $any")
    catch case _: WrongMethodTypeException => println("object result refused")
    println(neg.invoke(foo, 5).eq(null))
    println(neg.invoke(foo, 6).toString)
    println(neg.invoke(foo, 7).asInstanceOf[Int])
    val tag = l.findVirtual(classOf[Foo], "tag", MethodType.methodType(classOf[String], classOf[String]))
    val tagged: String = tag.invokeExact(foo, "a").asInstanceOf[String]
    val seq: CharSequence = tag.invoke(foo, "b").asInstanceOf[String]
    println(tagged + seq + (tag.invoke(foo, "c").asInstanceOf[String]: String).length)

    val ref = new IntRef(1)
    val vh = l.findVarHandle(classOf[IntRef], "elem", classOf[Int])
    println(vh.compareAndSet(ref, 1, 2))
    println(vh.getAndSet(ref, 3): Int)
    println(vh.get(ref): Int)
