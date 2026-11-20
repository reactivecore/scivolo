class C
object C:
  extension (c: C) def foo: String = "implicit scope"
object A:
  extension (c: C) def foo: String = "A"
object B:
  extension (s: String) def foo: String = "B"

import A.*
import B.*

@main def extensionalternatives = println(C().foo)
