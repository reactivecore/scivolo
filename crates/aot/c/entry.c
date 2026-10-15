// Starts the collector, then calls the compiled entry method with the first argument and prints
// its result.
// Temporary until #113 aot 3 (generated main): replaced by a `main` that runs the program's own.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern int scalai_entry(int n);
extern const int32_t scalai_aot_gc_ids[8];
extern void *scalai_aot_modules[];
extern const int64_t scalai_aot_modules_count;

void scalai_GC_init(void *low, void *high);
void scalai_GC_set_ids(const int32_t *ids);
void scalanative_GC_add_roots(void *low, void *high);

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s N\n", argv[0]);
        return 2;
    }
    void *low = &argc;
    scalai_GC_init(low, __builtin_frame_address(0));
    scalai_GC_set_ids(scalai_aot_gc_ids);
    scalanative_GC_add_roots(scalai_aot_modules, scalai_aot_modules + scalai_aot_modules_count);
    printf("%d\n", scalai_entry(atoi(argv[1])));
    return 0;
}
