object Outer:
  object Foo:
    class Wrap(a: String):
      def wow = s"$a - WOW!"
  object Bar:
    import Foo.Wrap
    def wrap(s: String) = Wrap(s)
    def wrapped(s: String) = Foo.Wrap(s)
