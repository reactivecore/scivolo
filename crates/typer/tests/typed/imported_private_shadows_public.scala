class Conf

class Bar(_conf: Conf):
  implicit val conf: Conf = _conf

class Foo(conf: Conf) extends Bar(conf)

class Test:
  def wildcard(foo: Foo) =
    import foo.*
    conf
  def named(foo: Foo) =
    import foo.conf
    conf
