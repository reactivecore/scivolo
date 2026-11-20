import scala.language.implicitConversions

class Sym(val name: String)
class Den(s: Sym):
  def enclosingClassNamed(n: String): String = s.name + n

object Syms:
  implicit def toDenot(s: Sym): Den = Den(s)

import Syms.*

class Namer { typer: Typer =>
  def owner = Sym("a")
  def enclosingClassNamed(name: String, span: Int): String = owner.enclosingClassNamed(name) + span
}

class Typer extends Namer

@main def selftypeviews = println(Typer().enclosingClassNamed("b", 1))
