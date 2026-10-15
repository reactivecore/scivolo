// Lookups the generated code calls: itable search and trait tests.
#include <stdint.h>

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
