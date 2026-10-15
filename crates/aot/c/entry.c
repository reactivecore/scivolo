// Starts the collector, then calls the compiled entry method with the first argument and prints
// its result.
// Temporary until #113 aot 3 (generated main): replaced by a `main` that runs the program's own.
#include <stdio.h>
#include <stdlib.h>

extern int scalai_entry(int n);
void scalai_aot_start_gc(void);

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s N\n", argv[0]);
        return 2;
    }
    scalai_aot_start_gc();
    printf("%d\n", scalai_entry(atoi(argv[1])));
    return 0;
}
