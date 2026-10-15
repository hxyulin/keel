/* Darwin SDK pointer annotations when bounds checking is unavailable. */
#ifndef __KEEL_PTRCHECK_H
#define __KEEL_PTRCHECK_H
#define __has_ptrcheck 0
#define __has_bounds_safety_attributes 0
#define __single
#define __unsafe_indexable
#define __null_terminated
#define __array_decay_discards_count_in_parameters
#define __counted_by(n)
#define __counted_by_or_null(n)
#define __sized_by(n)
#define __sized_by_or_null(n)
#define __ended_by(n)
#define __terminated_by(n)
#define __ptrcheck_abi_assume_single()
#define __ptrcheck_abi_assume_unsafe_indexable()
#define __unsafe_forge_single(type, ptr) ((type)(ptr))
#define __unsafe_forge_null_terminated(type, ptr) ((type)(ptr))
#define __unsafe_forge_terminated_by(type, ptr, end) ((type)(ptr))
#endif
