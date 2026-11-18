import scala.scalanative.reflect.Reflect
import scala.scalanative.reflect.annotation.EnableReflectiveInstantiation

@EnableReflectiveInstantiation
trait Plugin { def name: String }

trait Middle extends Plugin

class Plain extends Middle { def name = "plain" }

class Args(n: Int, s: String, xs: Array[Int], flag: Boolean) extends Plugin {
  def this(s: String) = this(1, s, Array(2, 3), true)
  protected def this(n: Int) = this(n, "protected", Array(), false)
  private def this(d: Double) = this(0, "private", Array(), false)
  def name = s"args $n $s ${xs.sum} $flag"
}

abstract class Partial extends Plugin

class Hidden private () extends Plugin { def name = "hidden" }

object Single extends Plugin { def name = "single" }

class Holder {
  object Inner extends Plugin { def name = "inner" }
  class Member extends Plugin { def name = "member" }
}

object Outer {
  object Nested extends Plugin { def name = "nested" }
  class Member extends Plugin { def name = "outer member" }
}

object reflectiveinstantiation {
  private def constructors(name: String): Unit =
    println(name + ": " + Reflect.lookupInstantiatableClass(name).map(
      _.declaredConstructors.map(_.parameterTypes.map(_.getName).mkString("(", ", ", ")")).mkString(" ")))

  private def instance(name: String): Unit =
    println(Reflect.lookupInstantiatableClass(name).map(_.newInstance().asInstanceOf[Plugin].name))

  private def module(name: String): Unit =
    println(name + ": " + Reflect.lookupLoadableModuleClass(name).map(_.loadModule() match {
      case p: Plugin => p.name
      case other     => String.valueOf(other)
    }))

  def main(args: Array[String]): Unit = {
    instance("Plain")
    instance("Outer$Member")
    constructors("Args")
    constructors("Partial")
    constructors("Plugin")
    constructors("Hidden")
    constructors("Holder$Member")
    val c = Reflect.lookupInstantiatableClass("Args").get
    val full = c.getConstructor(classOf[Int], classOf[String], classOf[Array[Int]], classOf[Boolean]).get
    println(full.newInstance(7, "seven", Array(1, 2), false).asInstanceOf[Plugin].name)
    println(c.getConstructor(classOf[String]).get.newInstance("one").asInstanceOf[Plugin].name)
    module("Single$")
    module("Outer$Nested$")
    module("Holder$Inner$")
    module("Plain")
  }
}
