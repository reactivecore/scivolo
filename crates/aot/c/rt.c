// Collector start-up, and lookups the generated code calls: itable search and trait tests.
#define _GNU_SOURCE
#include <pthread.h>
#include <stdint.h>

extern const int32_t scalai_aot_gc_ids[8];
extern void *scalai_aot_modules[];
extern const int64_t scalai_aot_modules_count;

void scalai_GC_init(void *low, void *high);
void scalai_GC_set_ids(const int32_t *ids);
void scalanative_GC_add_roots(void *low, void *high);

// Starts the collector on the calling thread's whole stack, with the module slots as roots.
void scalai_aot_start_gc(void) {
    char *low, *high;
#ifdef __APPLE__
    high = pthread_get_stackaddr_np(pthread_self());
    low = high - pthread_get_stacksize_np(pthread_self());
#else
    pthread_attr_t attr;
    void *addr;
    size_t size;
    pthread_getattr_np(pthread_self(), &attr);
    pthread_attr_getstack(&attr, &addr, &size);
    pthread_attr_destroy(&attr);
    low = addr;
    high = low + size;
#endif
    scalai_GC_init(low, high);
    scalai_GC_set_ids(scalai_aot_gc_ids);
    scalanative_GC_add_roots(scalai_aot_modules, scalai_aot_modules + scalai_aot_modules_count);
}

typedef struct {
    uint64_t key;
    void *method;
} ItableEntry;

// The method with `key` in an itable sorted by key, or NULL.
void *scalai_aot_itable(const ItableEntry *table, int64_t len, uint64_t key) {
    int64_t lo = 0, hi = len;
    while (lo < hi) {
        int64_t mid = lo + (hi - lo) / 2;
        if (table[mid].key < key)
            lo = mid + 1;
        else
            hi = mid;
    }
    return lo < len && table[lo].key == key ? table[lo].method : 0;
}

// Whether `trait` is among the `count` traits at `interfaces`.
int8_t scalai_aot_has_trait(void *const *interfaces, int32_t count, const void *trait) {
    for (int32_t i = 0; i < count; i++)
        if (interfaces[i] == trait)
            return 1;
    return 0;
}
