enum P[G[_], ->[_, _]]:
  case L[G[_], ->[_, _]](g: G[Int]) extends P[G, ->]

def f[G[_]](p: P[G, ?]): G[Int] = p match
  case P.L(g) => g
