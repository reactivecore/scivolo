class Quotes(val name: String)

transparent inline def quotes(using q: Quotes): q.type = q

class Printer[Q <: Quotes & Singleton](val q: Q):
  def show: String = q.name

def showTree(using Quotes): String = new Printer[quotes.type](quotes).show

@main def inlinedsingletons = println(showTree(using Quotes("a")))
