class Foo
class Bar

class JavaStreamClosures:
  def take(x: Foo) = ""
  def mapped(xs: java.util.stream.Stream[? <: Foo]) = xs.map(x => take(x))
  def lengths(xs: java.util.ArrayList[String]) = xs.stream.map(_.length).map(_.toString)
