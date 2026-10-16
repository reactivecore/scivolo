// Calls the Cranelift-compiled `fibo` of tests/programs/fib. The object writer prefixes every
// symbol with `_` on Mach-O, as C compilers do, so the Scala symbol `_S…` is `__S…` there.
#include <stdio.h>
#include <stdlib.h>

#ifdef __APPLE__
extern int fibo(void *self, int n) __asm__("__SM12fib$package$D4fiboiiEO");
#else
extern int fibo(void *self, int n) __asm__("_SM12fib$package$D4fiboiiEO");
#endif
extern int rt_selftest(int n);

int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 30;
    if (rt_selftest(41) != 42) {
        fprintf(stderr, "selftest failed\n");
        return 1;
    }
    printf("fib(%d) = %d\n", n, fibo(0, n));
    return 0;
}
