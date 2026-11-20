class Namer { typer: Typer =>
  class Completer(x: Int):
    def v = x
  def mk = Completer(1).v
}

class Typer extends Namer

@main def selftypeproxies = println(Typer().mk)
