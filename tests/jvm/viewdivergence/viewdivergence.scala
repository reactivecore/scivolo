import scala.language.implicitConversions

trait Foo[T]
class Ev[T]

class Selected
class Applied { def m(s: String): String = "own" }

class High:
  def sel: Foo[String] = { println("high"); new Foo[String] {} }
  def m(f: Foo[String]): String = "high"
class Low:
  def sel: Foo[String] = { println("low"); new Foo[String] {} }
  def m(f: Foo[String]): String = "low"

class Conv[T] extends Conversion[T, High] with Foo[Foo[T]]:
  def apply(x: T): High = new High

implicit val evString: Ev[String] = new Ev
implicit def evSelected(implicit c: Foo[Foo[String]]): Ev[Selected] = new Ev
implicit def evApplied(implicit c: Foo[Foo[String]]): Ev[Applied] = new Ev

trait LowPriority:
  implicit def selectedLow(x: Selected): Low = new Low
  implicit def appliedLow(x: Applied): Low = new Low
object Conversions extends LowPriority:
  implicit def conv[T](implicit ev: Ev[T]): Conv[T] = new Conv[T]

import Conversions.{appliedLow, conv, selectedLow}

def foo(): Foo[String] = new Foo[String] {}

object ViewDivergence:
  def main(args: Array[String]): Unit =
    val selected: Foo[String] = new Selected().sel
    println(selected != null)
    println(new Applied().m(foo()))
