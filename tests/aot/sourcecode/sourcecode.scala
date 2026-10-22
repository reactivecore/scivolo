//> using dep com.lihaoyi::sourcecode::0.4.4

package demo.trace

def log[T](v: sourcecode.Text[T])(using line: sourcecode.Line, enclosing: sourcecode.Enclosing): Unit =
  println(s"${line.value} ${enclosing.value}: ${v.source} = ${v.value}")

def nameOf(using n: sourcecode.Name): String = n.value
def pkgOf(using p: sourcecode.Pkg): String = p.value
def where(using e: sourcecode.Enclosing): String = e.value
def lineOf(using l: sourcecode.Line): Int = l.value
def fileOf(using f: sourcecode.File, n: sourcecode.FileName): String =
  s"${f.value.endsWith("/sourcecode.scala")} ${n.value}"

class Shop:
  val label = nameOf
  def total(prices: List[Int]): Int =
    val sum = prices.sum
    log(sum * 2)
    sum

object Shop:
  val shopName = nameOf
  lazy val pkg = pkgOf
  def inside: String = where

@main def run(): Unit =
  println(lineOf)
  println(fileOf)
  val myName = nameOf
  println(myName)
  println(Shop.shopName)
  println(Shop().label)
  println(Shop.pkg)
  println(where)
  println(Shop.inside)
  log(1 + 2)
  println(Shop().total(List(1, 2, 3)))
  println(sourcecode.Text(List(1).map(_ + 1)).source)
