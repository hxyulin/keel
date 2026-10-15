/* Keel compiler header. Individual guards support libc's __need_* requests. */
#if !defined(__need_size_t) && !defined(__need_ptrdiff_t) && !defined(__need_wchar_t) && !defined(__need_NULL) && !defined(__need_max_align_t) && !defined(__need_wint_t)
#define __KEEL_STDDEF_ALL
#endif
#if defined(__KEEL_STDDEF_ALL) || defined(__need_size_t)
#ifndef __KEEL_SIZE_T
#define __KEEL_SIZE_T
typedef __SIZE_TYPE__ size_t;
#endif
#undef __need_size_t
#endif
#if defined(__KEEL_STDDEF_ALL) || defined(__need_ptrdiff_t)
#ifndef __KEEL_PTRDIFF_T
#define __KEEL_PTRDIFF_T
typedef __PTRDIFF_TYPE__ ptrdiff_t;
#endif
#undef __need_ptrdiff_t
#endif
#if defined(__KEEL_STDDEF_ALL) || defined(__need_wchar_t)
#ifndef __KEEL_WCHAR_T
#define __KEEL_WCHAR_T
typedef __WCHAR_TYPE__ wchar_t;
#endif
#undef __need_wchar_t
#endif
#ifdef __need_wint_t
#ifndef __KEEL_WINT_T
#define __KEEL_WINT_T
typedef __WINT_TYPE__ wint_t;
#endif
#undef __need_wint_t
#endif
#if defined(__KEEL_STDDEF_ALL) || defined(__need_NULL)
#ifndef NULL
#define NULL ((void *)0)
#endif
#undef __need_NULL
#endif
#if defined(__KEEL_STDDEF_ALL) || defined(__need_max_align_t)
#ifndef __KEEL_MAX_ALIGN_T
#define __KEEL_MAX_ALIGN_T
#ifdef __APPLE__
typedef long double max_align_t;
#else
typedef struct { long long __integer; long double __floating; } max_align_t;
#endif
#endif
#undef __need_max_align_t
#endif
#ifdef __KEEL_STDDEF_ALL
#define offsetof(type, member) __builtin_offsetof(type, member)
#undef __KEEL_STDDEF_ALL
#endif
