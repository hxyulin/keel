#ifndef __KEEL_LIMITS_H
#define __KEEL_LIMITS_H
/* glibc must not search for another compiler limits.h after this one. */
#define _GCC_LIMITS_H_
#if __STDC_HOSTED__ && __has_include_next(<limits.h>)
#include_next <limits.h>
#endif
#undef CHAR_BIT
#undef SCHAR_MIN
#undef SCHAR_MAX
#undef UCHAR_MAX
#undef CHAR_MIN
#undef CHAR_MAX
#undef SHRT_MIN
#undef SHRT_MAX
#undef USHRT_MAX
#undef INT_MIN
#undef INT_MAX
#undef UINT_MAX
#undef LONG_MIN
#undef LONG_MAX
#undef ULONG_MAX
#undef LLONG_MIN
#undef LLONG_MAX
#undef ULLONG_MAX
#define CHAR_BIT __CHAR_BIT__
#define SCHAR_MAX __SCHAR_MAX__
#define SCHAR_MIN (-SCHAR_MAX - 1)
#define UCHAR_MAX (__SCHAR_MAX__ * 2 + 1)
#ifdef __CHAR_UNSIGNED__
#define CHAR_MIN 0
#define CHAR_MAX UCHAR_MAX
#else
#define CHAR_MIN SCHAR_MIN
#define CHAR_MAX SCHAR_MAX
#endif
#define SHRT_MAX __SHRT_MAX__
#define SHRT_MIN (-SHRT_MAX - 1)
#define USHRT_MAX (__SHRT_MAX__ * 2 + 1)
#define INT_MAX __INT_MAX__
#define INT_MIN (-INT_MAX - 1)
#define UINT_MAX (__INT_MAX__ * 2U + 1U)
#define LONG_MAX __LONG_MAX__
#define LONG_MIN (-LONG_MAX - 1L)
#define ULONG_MAX (__LONG_MAX__ * 2UL + 1UL)
#define LLONG_MAX __LONG_LONG_MAX__
#define LLONG_MIN (-LLONG_MAX - 1LL)
#define ULLONG_MAX (__LONG_LONG_MAX__ * 2ULL + 1ULL)
#endif
