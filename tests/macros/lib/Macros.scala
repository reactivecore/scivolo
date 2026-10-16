package macrolib

import scala.quoted.*

object Macros:
  inline def answer: Int = ${ answerImpl }
  def answerImpl(using Quotes): Expr[Int] = '{ 42 }

  inline def twice(inline x: Int): Int = ${ twiceImpl('x) }
  def twiceImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ $x + $x }

  inline def greet(name: String): String = ${ greetImpl('name) }
  def greetImpl(name: Expr[String])(using Quotes): Expr[String] = '{ "Hello, " + $name + "!" }

  inline def pair[T](x: T): (T, T) = ${ pairImpl('x) }
  def pairImpl[T: Type](x: Expr[T])(using Quotes): Expr[(T, T)] = '{ ($x, $x) }

  inline def listOf[T](x: T, y: T): List[T] = ${ listOfImpl('x, 'y) }
  def listOfImpl[T: Type](x: Expr[T], y: Expr[T])(using Quotes): Expr[List[T]] = '{ List[T]($x, $y) }

  inline def incremented(x: Int): Int = ${ incrementedImpl('x) }
  def incrementedImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ Helpers.inc($x) }

  inline def logged[T](x: T): T = ${ loggedImpl('x) }
  def loggedImpl[T: Type](x: Expr[T])(using Quotes): Expr[T] = '{ val v = $x; println(v); v }

  inline def plusOneTwice(inline x: Int): Int = ${ twiceImpl('{ x + 1 }) }

  inline def wrapped[T](x: T): List[T] = ${ wrappedImpl('x) }
  def wrappedImpl[T: Type](x: Expr[T])(using Quotes): Expr[List[T]] = typed('{ List($x) })(using Type.of[List[T]])
  def typed[U: Type](e: Expr[U])(using Quotes): Expr[U] = '{ identity[U]($e) }

  inline def fail: Int = ${ failImpl }
  def failImpl(using Quotes): Expr[Int] = throw new RuntimeException("the macro failed")

object Helpers:
  inline def inc(x: Int): Int = x + 1
