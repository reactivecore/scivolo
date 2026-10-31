//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Terms.*

trait Logger:
  def log(a: String): Unit

object Messages:
  def logMessage(a: String): Unit = println(a)
  def count = 1

object Exports:
  val a = code { object Observer { export Messages.count } }
  val b = code(new Logger { export Messages.{logMessage => log} })
  val c = code(new Logger { export Messages.{count as n, logMessage => log, given} })
