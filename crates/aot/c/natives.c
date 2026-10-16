// Natives the VM implements in Rust, for executables: the current thread, stack guards (no-ops:
// stack.c guards the stack), and the LLVM intrinsics Scala Native's code calls.
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

// Temporary until #115 (resources): no classpath resources are embedded.
int32_t scalanative_resource_get_embedded_size(void) { return 0; }
int32_t scalanative_resource_get_path_length(int32_t id) { return 0; }
void *scalanative_resource_get_path_ptr(int32_t id) { return 0; }
int32_t scalanative_resource_get_content_length(int32_t id) { return 0; }
void *scalanative_resource_get_content_ptr(int32_t id) { return 0; }

#define UNARY(name, fn)                                                                            \
    double scalai_aot_llvm_##name##_f64(double x) { return fn(x); }                                \
    float scalai_aot_llvm_##name##_f32(float x) { return fn##f(x); }
#define BINARY(name, fn)                                                                           \
    double scalai_aot_llvm_##name##_f64(double x, double y) { return fn(x, y); }                   \
    float scalai_aot_llvm_##name##_f32(float x, float y) { return fn##f(x, y); }
UNARY(ceil, ceil)
UNARY(floor, floor)
UNARY(sqrt, sqrt)
UNARY(fabs, fabs)
UNARY(log, log)
UNARY(log10, log10)
UNARY(log2, log2)
UNARY(exp, exp)
UNARY(exp2, exp2)
UNARY(sin, sin)
UNARY(cos, cos)
UNARY(trunc, trunc)
UNARY(rint, rint)
UNARY(round, round)
UNARY(nearbyint, nearbyint)
BINARY(pow, pow)
BINARY(maxnum, fmax)
BINARY(minnum, fmin)
BINARY(copysign, copysign)
double scalai_aot_frem_f64(double x, double y) { return fmod(x, y); }
float scalai_aot_frem_f32(float x, float y) { return fmodf(x, y); }
int32_t scalai_aot_llvm_ctlz_i32(int32_t x, int8_t undef) { return x ? __builtin_clz(x) : 32; }
int64_t scalai_aot_llvm_ctlz_i64(int64_t x, int8_t undef) { return x ? __builtin_clzll(x) : 64; }
int32_t scalai_aot_llvm_cttz_i32(int32_t x, int8_t undef) { return x ? __builtin_ctz(x) : 32; }
int64_t scalai_aot_llvm_cttz_i64(int64_t x, int8_t undef) { return x ? __builtin_ctzll(x) : 64; }
int32_t scalai_aot_llvm_ctpop_i32(int32_t x) { return __builtin_popcount(x); }
int64_t scalai_aot_llvm_ctpop_i64(int64_t x) { return __builtin_popcountll(x); }
