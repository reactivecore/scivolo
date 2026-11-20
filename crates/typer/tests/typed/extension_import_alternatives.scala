class A
class B

object ExtensionA:
  extension (self: A) def id = "A"

object ExtensionB:
  extension (self: B) def id = "B"

object One:
  extension (s: String) def wow: Int = 1

object Two:
  extension (i: Int) def wow: Int = 2

object Conflicting:
  extension (i: Int) def wow: Int = 3

val Alias = Two

object Main:
  def wildcards =
    import ExtensionB.*
    import ExtensionA.*
    (A().id, B().id)

  def named =
    import One.wow
    import Two.wow
    import Conflicting.*
    (5.wow, "five".wow)

  def sameObject =
    import Two.*
    import Alias.*
    5.wow

class C:
  extension (s: String)
    def f = "hello"
    def e =
      import this.f as hw
      hw
