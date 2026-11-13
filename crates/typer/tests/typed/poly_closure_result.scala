def foo[R](f: [A] => (o: List[A]) => R): R = f(Nil)

def literal = foo([A] => (o: List[A]) => 42)
def matched = foo([A] => (o: List[A]) => { o match { case _ => 42 } })
def defined = foo[Int]([A] => (o: List[A]) => 42)
