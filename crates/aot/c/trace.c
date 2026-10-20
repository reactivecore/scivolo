// Scala Native's unwinder interface over a frame-pointer walk, naming frames by the code-address
// table `scivolo_aot_frames`. As in crates/vm/src/natives/unwind.rs, `get_context` snapshots the
// frames and a cursor is `[generation, index]` into the snapshot.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    uint32_t start, end;
    int32_t line;
    uint32_t file;
} Line;

// Names are offsets into `scivolo_aot_names`.
typedef struct {
    const char *code;
    int64_t size, class_name, method;
    int64_t first, count;
    int64_t builds_throwable;
} Frame;

extern const Frame scivolo_aot_frames[];
// The lines of all methods; a method's are `count` from `first`.
extern const Line scivolo_aot_lines[];
// Each name's length in two UTF-16 units, then its units.
extern const uint16_t scivolo_aot_names[];
extern const struct {
    int64_t frames_count;
    void *string, *char_array;
    int64_t string_size, unknown;
} scivolo_aot_trace_info;
// The offsets of a class's name, `String`'s `value` and `count`, and the array header size.
extern const int32_t scivolo_aot_layout[4];
void *scalanative_GC_alloc(void *info, uint64_t size);
void *scalanative_GC_alloc_array(void *info, uint64_t length, uint64_t stride);
#define scivolo_aot_frames_count scivolo_aot_trace_info.frames_count
extern char *scivolo_aot_stack_high;

#define MAX_FRAMES 1025
#define SKIPPABLE 32

static const Frame **by_address = 0;
static uint64_t generation = 0;
static const char *ips[MAX_FRAMES + SKIPPABLE];
static int64_t ip_count = 0;

static int by_start(const void *a, const void *b) {
    const char *x = (*(const Frame *const *)a)->code, *y = (*(const Frame *const *)b)->code;
    return x < y ? -1 : x > y;
}

// The method whose code holds `pc`, if compiled.
static const Frame *frame_at(const char *pc) {
    int64_t n = scivolo_aot_frames_count;
    if (by_address == 0) {
        by_address = malloc(sizeof(Frame *) * (n > 0 ? n : 1));
        for (int64_t i = 0; i < n; i++)
            by_address[i] = &scivolo_aot_frames[i];
        qsort(by_address, n, sizeof(Frame *), by_start);
    }
    int64_t lo = 0, hi = n;
    while (lo < hi) {
        int64_t mid = lo + (hi - lo) / 2;
        if (by_address[mid]->code <= pc)
            lo = mid + 1;
        else
            hi = mid;
    }
    if (lo == 0)
        return 0;
    const Frame *f = by_address[lo - 1];
    return pc < f->code + f->size ? f : 0;
}

int64_t scalanative_unwind_sizeof_context(void) { return 16; }
int64_t scalanative_unwind_sizeof_cursor(void) { return 16; }
int scalanative_unw_reg_ip(void) { return 0; }
int scalanative_unwind_get_proc_name_by_ip(void *ip, char *buf, int64_t len, void *offset) { return -1; }

// Snapshots the compiled frames from the caller outwards, past the constructors and
// `fillInStackTrace` of the throwable being built.
int scalanative_unwind_get_context(void *context) {
    void **fp = __builtin_frame_address(0);
    const Frame *frames[MAX_FRAMES + SKIPPABLE];
    int64_t n = 0;
    while (fp != 0 && n < MAX_FRAMES + SKIPPABLE) {
        const char *ip = fp[1];
        const Frame *f = frame_at(ip - 1);
        if (f != 0) {
            frames[n] = f;
            ips[n++] = ip;
        }
        void **next = fp[0];
        if (next <= fp || (char *)next >= scivolo_aot_stack_high || (uintptr_t)next % 16 != 0)
            break;
        fp = next;
    }
    int64_t start = 0;
    while (start + 1 < n && frames[start + 1]->builds_throwable)
        start++;
    ip_count = n - start < MAX_FRAMES ? n - start : MAX_FRAMES;
    for (int64_t i = 0; i < ip_count; i++)
        ips[i] = ips[start + i];
    *(uint64_t *)context = ++generation;
    return 0;
}

int scalanative_unwind_init_local(void *cursor, void *context) {
    ((uint64_t *)cursor)[0] = *(uint64_t *)context;
    ((uint64_t *)cursor)[1] = 0;
    return 0;
}

int scalanative_unwind_step(void *cursor) {
    uint64_t *c = cursor;
    uint64_t next = c[1] + 1;
    c[1] = next;
    return c[0] == generation && next < (uint64_t)ip_count;
}

int scalanative_unwind_get_reg(void *cursor, int reg, void *value) {
    uint64_t *c = cursor;
    if (c[0] != generation || c[1] >= (uint64_t)ip_count)
        return -1;
    *(const char **)value = ips[c[1]];
    return 0;
}

// The class name, method name, file name (or null) and line (or -1) of the call at return
// address `ip`, into `out`.
// A new `java.lang.String` of the name at offset `at` of `scivolo_aot_names`.
static void *name(int64_t at) {
    const uint16_t *p = &scivolo_aot_names[at];
    uint32_t length = p[0] | (uint32_t)p[1] << 16;
    const int32_t *layout = scivolo_aot_layout;
    char *chars = scalanative_GC_alloc_array(scivolo_aot_trace_info.char_array, length, 2);
    memcpy(chars + layout[3], p + 2, 2 * (size_t)length);
    char *s = scalanative_GC_alloc(scivolo_aot_trace_info.string, scivolo_aot_trace_info.string_size);
    *(char **)(s + layout[1]) = chars;
    *(int32_t *)(s + layout[2]) = (int32_t)length;
    return s;
}

// The class name, method name, file name (or null) and line (or -1) of the call at return
// address `ip`, into `out`, which the collector scans.
void scivolo_aot_frame_info(const char *ip, void **out) {
    const Frame *f = frame_at(ip - 1);
    out[0] = out[1] = out[2] = 0;
    *(int64_t *)&out[3] = -1;
    int64_t unknown = scivolo_aot_trace_info.unknown;
    out[0] = name(f ? f->class_name : unknown);
    out[1] = name(f ? f->method : unknown);
    if (f == 0)
        return;
    uint32_t at = (uint32_t)(ip - 1 - f->code);
    const Line *lines = &scivolo_aot_lines[f->first];
    for (int64_t i = 0; i < f->count; i++)
        if (lines[i].start <= at && at < lines[i].end) {
            out[2] = name(lines[i].file);
            *(int64_t *)&out[3] = lines[i].line;
            return;
        }
}
