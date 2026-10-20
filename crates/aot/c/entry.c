// Starts the collector, then calls the compiled entry method with the first argument and prints
// its result: executables of single methods, for tests.
#include <stdio.h>
#include <stdlib.h>

extern int scivolo_entry(int n);
void scivolo_aot_start_gc(void);

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s N\n", argv[0]);
        return 2;
    }
    scivolo_aot_start_gc();
    printf("%d\n", scivolo_entry(atoi(argv[1])));
    return 0;
}
