enum Op[A]:
  case Dup[T]() extends Op[(T, T)]

def t2[A](o: Op[A]) = o match
  case o: Op.Dup[u] =>
    summon[A =:= (u, u)]
    ()

def t3[A](o: Op[A]): Unit = o match
  case o: Op.Dup[u] =>
    summon[A =:= (u, u)]
    ()

def t4[A](o: Op[A]) = o match
  case o: Op.Dup[u] =>
    summon[A =:= (u, u)](using <:<.refl[A])
    ()

def t5[A](o: Op[A]): Unit = o match
  case o: Op.Dup[u] =>
    summon[A =:= (u, u)](using <:<.refl[A])
    ()
