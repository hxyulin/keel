.text
.globl core_rcl
core_rcl:
bt rsi,0
mov rax,rdi
rcl rax,1
setc byte ptr [rdx]
ret
.globl core_rcr
core_rcr:
bt rsi,0
mov rax,rdi
rcr rax,1
setc byte ptr [rdx]
ret
.globl core_loop
core_loop:
mov rcx,rdi
xor eax,eax
.Lloop_top:
add eax,1
loop .Lloop_top
ret
.globl core_loope
core_loope:
mov rcx,rdi
xor eax,eax
.Lloope_top:
add eax,1
cmp eax,1
loope .Lloope_top
ret
.globl core_loopne
core_loopne:
mov rcx,rdi
xor eax,eax
.Lloopne_top:
add eax,1
cmp eax,2
loopne .Lloopne_top
ret
.globl core_jecxz
core_jecxz:
mov rcx,rdi
jecxz .Ljecxz_zero
mov eax,1
ret
.Ljecxz_zero:
mov eax,2
ret
.globl core_jrcxz
core_jrcxz:
mov rcx,rdi
jrcxz .Ljrcxz_zero
mov eax,1
ret
.Ljrcxz_zero:
mov eax,2
ret
.globl core_copy
core_copy:
pushfq
cld
mov rcx,rdx
rep movsb
popfq
ret
.globl core_copy_df
core_copy_df:
pushfq
std
pushfq
cld
mov rcx,rdx
rep movsb
popfq
pushfq
pop rax
shr rax,10
and eax,1
cld
popfq
ret
.globl core_stosq
core_stosq:
pushfq
cld
mov rax,rsi
mov rcx,rdx
rep stosq
popfq
ret
.globl core_lodsb
core_lodsb:
pushfq
cld
mov rsi,rdi
lodsb
movzx eax,al
popfq
ret
.globl core_cmps
core_cmps:
pushfq
cld
mov rcx,rdx
repe cmpsb
setz al
movzx eax,al
popfq
ret
.globl core_rep_lods
core_rep_lods:
pushfq
cld
mov rcx,rsi
mov rsi,rdi
rep lodsb
movzx eax,al
popfq
ret
.globl core_scas
core_scas:
pushfq
cld
mov r8,rdi
mov rcx,rsi
mov al,dl
repne scasb
mov rax,rdi
sub rax,r8
popfq
ret
.globl core_xlat
core_xlat:
push rbx
mov rbx,rdi
mov al,sil
xlatb
movzx eax,al
pop rbx
ret
.globl core_cx8
core_cx8:
push rbx
mov r8,rdx
mov eax,esi
shr rsi,32
mov edx,esi
mov ebx,r8d
shr r8,32
mov ecx,r8d
lock cmpxchg8b qword ptr [rdi]
sete al
movzx eax,al
pop rbx
ret
.globl core_cwd
core_cwd:
mov ax,di
cwd
movzx eax,dx
ret
.globl core_enter
core_enter:
enter 0,0
leave
mov eax,37
ret
.globl core_flush
core_flush:
clflush byte ptr [rdi]
ret
.globl core_addr32_load
core_addr32_load:
mov eax,dword ptr [edi]
ret
.globl core_addr32_store
core_addr32_store:
mov dword ptr [edi],esi
ret
.section .note.GNU-stack,"",@progbits
