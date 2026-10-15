#ifndef __KEEL_STDATOMIC_H
#define __KEEL_STDATOMIC_H
#include <stddef.h>
#include <stdint.h>
typedef enum { memory_order_relaxed = __ATOMIC_RELAXED, memory_order_consume = __ATOMIC_CONSUME, memory_order_acquire = __ATOMIC_ACQUIRE, memory_order_release = __ATOMIC_RELEASE, memory_order_acq_rel = __ATOMIC_ACQ_REL, memory_order_seq_cst = __ATOMIC_SEQ_CST } memory_order;
typedef _Atomic(_Bool) atomic_bool;
typedef _Atomic(char) atomic_char;
typedef _Atomic(signed char) atomic_schar;
typedef _Atomic(unsigned char) atomic_uchar;
typedef _Atomic(short) atomic_short;
typedef _Atomic(unsigned short) atomic_ushort;
typedef _Atomic(int) atomic_int;
typedef _Atomic(unsigned int) atomic_uint;
typedef _Atomic(long) atomic_long;
typedef _Atomic(unsigned long) atomic_ulong;
typedef _Atomic(long long) atomic_llong;
typedef _Atomic(unsigned long long) atomic_ullong;
typedef _Atomic(__CHAR16_TYPE__) atomic_char16_t;
typedef _Atomic(__CHAR32_TYPE__) atomic_char32_t;
typedef _Atomic(__WCHAR_TYPE__) atomic_wchar_t;
typedef _Atomic(intptr_t) atomic_intptr_t;
typedef _Atomic(uintptr_t) atomic_uintptr_t;
typedef _Atomic(size_t) atomic_size_t;
typedef _Atomic(ptrdiff_t) atomic_ptrdiff_t;
typedef _Atomic(intmax_t) atomic_intmax_t;
typedef _Atomic(uintmax_t) atomic_uintmax_t;
typedef _Atomic(int_least8_t) atomic_int_least8_t;
typedef _Atomic(uint_least8_t) atomic_uint_least8_t;
typedef _Atomic(int_fast8_t) atomic_int_fast8_t;
typedef _Atomic(uint_fast8_t) atomic_uint_fast8_t;
typedef _Atomic(int_least16_t) atomic_int_least16_t;
typedef _Atomic(uint_least16_t) atomic_uint_least16_t;
typedef _Atomic(int_fast16_t) atomic_int_fast16_t;
typedef _Atomic(uint_fast16_t) atomic_uint_fast16_t;
typedef _Atomic(int_least32_t) atomic_int_least32_t;
typedef _Atomic(uint_least32_t) atomic_uint_least32_t;
typedef _Atomic(int_fast32_t) atomic_int_fast32_t;
typedef _Atomic(uint_fast32_t) atomic_uint_fast32_t;
typedef _Atomic(int_least64_t) atomic_int_least64_t;
typedef _Atomic(uint_least64_t) atomic_uint_least64_t;
typedef _Atomic(int_fast64_t) atomic_int_fast64_t;
typedef _Atomic(uint_fast64_t) atomic_uint_fast64_t;
#define ATOMIC_BOOL_LOCK_FREE __GCC_ATOMIC_BOOL_LOCK_FREE
#define ATOMIC_CHAR_LOCK_FREE __GCC_ATOMIC_CHAR_LOCK_FREE
#define ATOMIC_CHAR16_T_LOCK_FREE __GCC_ATOMIC_CHAR16_T_LOCK_FREE
#define ATOMIC_CHAR32_T_LOCK_FREE __GCC_ATOMIC_CHAR32_T_LOCK_FREE
#define ATOMIC_WCHAR_T_LOCK_FREE __GCC_ATOMIC_WCHAR_T_LOCK_FREE
#define ATOMIC_SHORT_LOCK_FREE __GCC_ATOMIC_SHORT_LOCK_FREE
#define ATOMIC_INT_LOCK_FREE __GCC_ATOMIC_INT_LOCK_FREE
#define ATOMIC_LONG_LOCK_FREE __GCC_ATOMIC_LONG_LOCK_FREE
#define ATOMIC_LLONG_LOCK_FREE __GCC_ATOMIC_LLONG_LOCK_FREE
#define ATOMIC_POINTER_LOCK_FREE __GCC_ATOMIC_POINTER_LOCK_FREE
#define ATOMIC_VAR_INIT(value) (value)
#define atomic_init(p, value) __c11_atomic_init(p, value)
#define atomic_is_lock_free(p) __atomic_is_lock_free(sizeof(*(p)), (p))
#define atomic_thread_fence(order) __c11_atomic_thread_fence(order)
#define atomic_signal_fence(order) __c11_atomic_signal_fence(order)
#define atomic_load(p) atomic_load_explicit(p, memory_order_seq_cst)
#define atomic_load_explicit(p, order) __c11_atomic_load(p, order)
#define atomic_store(p, value) atomic_store_explicit(p, value, memory_order_seq_cst)
#define atomic_store_explicit(p, value, order) __c11_atomic_store(p, value, order)
#define atomic_exchange(p, value) atomic_exchange_explicit(p, value, memory_order_seq_cst)
#define atomic_exchange_explicit(p, value, order) __c11_atomic_exchange(p, value, order)
#define atomic_fetch_add(p, value) atomic_fetch_add_explicit(p, value, memory_order_seq_cst)
#define atomic_fetch_add_explicit(p, value, order) __c11_atomic_fetch_add(p, value, order)
#define atomic_fetch_sub(p, value) atomic_fetch_sub_explicit(p, value, memory_order_seq_cst)
#define atomic_fetch_sub_explicit(p, value, order) __c11_atomic_fetch_sub(p, value, order)
#define atomic_fetch_and(p, value) atomic_fetch_and_explicit(p, value, memory_order_seq_cst)
#define atomic_fetch_and_explicit(p, value, order) __c11_atomic_fetch_and(p, value, order)
#define atomic_fetch_or(p, value) atomic_fetch_or_explicit(p, value, memory_order_seq_cst)
#define atomic_fetch_or_explicit(p, value, order) __c11_atomic_fetch_or(p, value, order)
#define atomic_fetch_xor(p, value) atomic_fetch_xor_explicit(p, value, memory_order_seq_cst)
#define atomic_fetch_xor_explicit(p, value, order) __c11_atomic_fetch_xor(p, value, order)
#define atomic_compare_exchange_strong(p, expected, desired) atomic_compare_exchange_strong_explicit(p, expected, desired, memory_order_seq_cst, memory_order_seq_cst)
#define atomic_compare_exchange_strong_explicit(p, expected, desired, success, failure) __c11_atomic_compare_exchange_strong(p, expected, desired, success, failure)
#define atomic_compare_exchange_weak(p, expected, desired) atomic_compare_exchange_weak_explicit(p, expected, desired, memory_order_seq_cst, memory_order_seq_cst)
#define atomic_compare_exchange_weak_explicit(p, expected, desired, success, failure) __c11_atomic_compare_exchange_weak(p, expected, desired, success, failure)
typedef struct { _Atomic(_Bool) __value; } atomic_flag;
#define ATOMIC_FLAG_INIT { 0 }
#define atomic_flag_test_and_set(p) atomic_flag_test_and_set_explicit(p, memory_order_seq_cst)
#define atomic_flag_test_and_set_explicit(p, order) __c11_atomic_exchange(&(p)->__value, 1, order)
#define atomic_flag_clear(p) atomic_flag_clear_explicit(p, memory_order_seq_cst)
#define atomic_flag_clear_explicit(p, order) __c11_atomic_store(&(p)->__value, 0, order)
#endif
