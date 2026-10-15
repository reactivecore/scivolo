// Calls the compiled entry method with the first argument (default 20) and prints its result.
#include <stdio.h>
#include <stdlib.h>

extern int scalai_entry(int n);

int main(int argc, char **argv) {
    printf("%d\n", scalai_entry(argc > 1 ? atoi(argv[1]) : 20));
    return 0;
}
