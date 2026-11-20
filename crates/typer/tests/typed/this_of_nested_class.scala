trait Outer:
  abstract class Inner[+T]:
    def same = this
  class Leaf:
    def me = this

object O:
  class Inner:
    def me = this
