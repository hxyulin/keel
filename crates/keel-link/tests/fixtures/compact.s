// Same frame/LSDA contract as keel-asm/tests/compact.rs.

.subsections_via_symbols
.text
.globl _catch_compact
_catch_compact:
Lbegin:
.cfi_startproc
.cfi_personality 155,___gxx_personality_v0
.cfi_lsda 16,Llsda
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x30,-8
.cfi_offset x29,-16
stp x20,x19,[sp,#-16]!
.cfi_offset x19,-24
.cfi_offset x20,-32
stp d9,d8,[sp,#-16]!
.cfi_offset d8,-40
.cfi_offset d9,-48
mov w19,#11
mov w20,#31
fmov d8,#2.0
fmov d9,#3.0
Lfrom: bl _compact_throw
Lto: brk #1
Lpad:
bl ___cxa_begin_catch
bl ___cxa_end_catch
add w0,w19,w20
fcvtzs w1,d8
add w0,w0,w1
fcvtzs w1,d9
add w0,w0,w1
ldp d9,d8,[sp],#16
ldp x20,x19,[sp],#16
ldp x29,x30,[sp],#16
ret
.cfi_endproc
_compact_throw:
.cfi_startproc
stp x29,x30,[sp,#-16]!
mov x29,sp
.cfi_def_cfa x29,16
.cfi_offset x30,-8
.cfi_offset x29,-16
stp x20,x19,[sp,#-16]!
.cfi_offset x19,-24
.cfi_offset x20,-32
stp d9,d8,[sp,#-16]!
.cfi_offset d8,-40
.cfi_offset d9,-48
mov w19,#0
mov w20,#0
fmov d8,#9.0
fmov d9,#10.0
bl _thrower
brk #2
.cfi_endproc
.section __TEXT,__gcc_except_tab
.p2align 2
GCC_except_table_compact:
Llsda:
.byte 255,155
.uleb128 Lttbase-Lttref
Lttref:
.byte 1
.uleb128 Lcsend-Lcs
Lcs:
.uleb128 Lfrom-Lbegin
.uleb128 Lto-Lfrom
.uleb128 Lpad-Lbegin
.byte 1
Lcsend:
.byte 1,0
.p2align 2
Ltype:
.long __ZTIi@GOT-Ltype
Lttbase:
