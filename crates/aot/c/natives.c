// Natives the VM implements in Rust, for executables: the current thread, stack guards, the
// unwinder, and the LLVM intrinsics Scala Native's code calls.
// Temporary until #113 aot 4 (stack traces) and aot 5 (intrinsics, natives): the unwinder
// reports no frames, stack guards are absent, and only the intrinsics hello world needs exist.
#include <math.h>
#include <stdint.h>

// The current `java.lang.Thread` and native thread objects; a collector root.
void *scalai_aot_thread[2] = {0, 0};

void scalanative_assignCurrentThread(void *thread, void *native) {
    scalai_aot_thread[0] = thread;
    scalai_aot_thread[1] = native;
}
void *scalanative_currentThread(void) { return scalai_aot_thread[0]; }
void *scalanative_currentNativeThread(void) { return scalai_aot_thread[1]; }
void scalanative_setupCurrentThreadInfo(void *thread) {}

void scalanative_StackOverflowGuards_setup(int main) {}
void scalanative_StackOverflowGuards_reset(void) {}
void scalanative_StackOverflowGuards_close(void) {}
int64_t scalanative_StackOverflowGuards_size(void) { return 0; }
int64_t scalanative_Throwable_sizeOfExceptionWrapper(void) { return 0; }

int64_t scalanative_unwind_sizeof_context(void) { return 16; }
int64_t scalanative_unwind_sizeof_cursor(void) { return 16; }
int scalanative_unwind_get_context(void *context) { return 0; }
int scalanative_unwind_init_local(void *cursor, void *context) { return 0; }
int scalanative_unwind_step(void *cursor) { return 0; }
int scalanative_unwind_get_reg(void *cursor, int reg, void *value) { return 0; }
int scalanative_unw_reg_ip(void) { return 0; }
int scalanative_unwind_get_proc_name_by_ip(void *ip, char *buf, int64_t len, void *offset) { return -1; }

double scalai_aot_llvm_ceil_f64(double x) { return ceil(x); }
double scalai_aot_llvm_floor_f64(double x) { return floor(x); }
double scalai_aot_llvm_sqrt_f64(double x) { return sqrt(x); }
int32_t scalai_aot_llvm_ctlz_i32(int32_t x, int8_t undef) { return x ? __builtin_clz(x) : 32; }
int64_t scalai_aot_llvm_ctlz_i64(int64_t x, int8_t undef) { return x ? __builtin_clzll(x) : 64; }
int32_t scalai_aot_llvm_cttz_i32(int32_t x, int8_t undef) { return x ? __builtin_ctz(x) : 32; }
int64_t scalai_aot_llvm_cttz_i64(int64_t x, int8_t undef) { return x ? __builtin_ctzll(x) : 64; }
int32_t scalai_aot_llvm_ctpop_i32(int32_t x) { return __builtin_popcount(x); }
int64_t scalai_aot_llvm_ctpop_i64(int64_t x) { return __builtin_popcountll(x); }
