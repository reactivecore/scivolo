def foo[R](f: [A] => (o: List[A]) => R): R = f(Nil)

def literal = foo([A] => (o: List[A]) => 42)
def matched = foo([A] => (o: List[A]) => { o match { case _ => 42 } })
def defined = foo[Int]([A] => (o: List[A]) => 42)

trait Box { type T; val v: T }
def dep[R](f: (b: Box) => (b.T, R)): R = ???
def named[R](f: (b: Box) => List[R]): R = ???

def dependent = dep(b => (b.v, 1))
def empty = named(b => Nil)
