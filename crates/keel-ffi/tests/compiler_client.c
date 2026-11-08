/* Compiler-facing coverage: floats, checked arithmetic, records, varargs and atomics. */
#include "keel.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static keel_module *m;
static void check(keel_status s, int line) {
    if (s != KEEL_OK) { fprintf(stderr, "compiler_client.c:%d: %s\n", line, keel_module_error(m)); exit(1); }
}
#define CHECK(call) check((call), __LINE__)
static keel_value iconst(keel_block b, keel_type t, int64_t n) {
    keel_value v; CHECK(keel_build_iconst(m,b,t,n,&v)); return v;
}
static keel_value binary(keel_block b, keel_binary_op op, keel_value a, keel_value c) {
    keel_value v; CHECK(keel_build_binary(m,b,op,a,c,&v)); return v;
}
static keel_value convert(keel_block b, keel_convert_op op, keel_type t, keel_value a) {
    keel_value v; CHECK(keel_build_convert(m,b,op,t,a,&v)); return v;
}
static keel_signature signature(const keel_abi_value *params, size_t n, const keel_abi_value *ret,
                                size_t nr, uint32_t flags) {
    keel_signature_options options = { KEEL_CC_C, flags, KEEL_X86_VECTOR_ABI_DEFAULT };
    keel_signature s; CHECK(keel_signature_create_full(m,params,n,ret,nr,options,&s)); return s;
}
static keel_function start(const char *name, keel_signature s, const keel_abi_value *params,
                           size_t n, keel_block *b, keel_value *values) {
    keel_function f; CHECK(keel_module_add_function_with_signature(m,(const uint8_t *)name,strlen(name),s,&f));
    CHECK(keel_block_create(m,f,b));
    for (size_t i=0;i<n;++i) CHECK(keel_block_add_parameter(m,*b,params[i].type,&values[i]));
    return f;
}
static keel_symbol external(const char *name, keel_signature s) {
    keel_symbol symbol; CHECK(keel_module_declare_function(m,(const uint8_t *)name,strlen(name),s,&symbol)); return symbol;
}
static keel_value load(keel_block b, keel_type t, keel_value p) {
    const keel_memory memory = {1,0}; keel_value v; CHECK(keel_build_load(m,b,t,p,memory,&v)); return v;
}
static void store(keel_block b, keel_value v, keel_value p) {
    const keel_memory memory = {1,0}; CHECK(keel_build_store(m,b,v,p,memory));
}
static keel_value offset(keel_block b, keel_value p, int64_t n) {
    keel_value v, bytes=iconst(b,KEEL_I64,n); CHECK(keel_build_ptradd(m,b,p,bytes,&v)); return v;
}
static void intrinsic(keel_block b, const char *name, const keel_value *args, size_t n) {
    CHECK(keel_build_intrinsic(m,b,(const uint8_t *)name,strlen(name),args,n,NULL,0));
}

int main(int argc, char **argv) {
    if (argc != 4) return 1;
    keel_target target;
    if (!strcmp(argv[1],"darwin-arm64")) target=KEEL_AARCH64_APPLE_DARWIN;
    else if (!strcmp(argv[1],"linux-arm64")) target=KEEL_AARCH64_LINUX_GNU;
    else if (!strcmp(argv[1],"linux-x86_64")) target=KEEL_X86_64_LINUX_GNU;
    else return 1;
    m=keel_module_create();
    const keel_abi_value i64={KEEL_I64,KEEL_ABI_NONE,KEEL_ABI_NORMAL,{0,0}};
    const keel_abi_value i32={KEEL_I32,KEEL_ABI_NONE,KEEL_ABI_NORMAL,{0,0}};
    const keel_abi_value f64={KEEL_F64,KEEL_ABI_NONE,KEEL_ABI_NORMAL,{0,0}};
    const keel_abi_value ptr={KEEL_PTR,KEEL_ABI_NONE,KEEL_ABI_NORMAL,{0,0}};
    keel_block b;
    keel_value args[4], v, results[2];
    const keel_abi_value checked_params[]={i64,i64,ptr};
    start("checked_add",signature(checked_params,3,&i64,1,0),checked_params,3,&b,args);
    CHECK(keel_build_overflow(m,b,KEEL_SADD_OVERFLOW,args[0],args[1],results,2));
    store(b,results[1],args[2]); CHECK(keel_build_return(m,b,results,1));

    const keel_abi_value float_params[]={f64,f64};
    start("float_math",signature(float_params,2,&f64,1,0),float_params,2,&b,args);
    CHECK(keel_build_unary(m,b,KEEL_FABS,args[1],&v));
    keel_value two; CHECK(keel_build_fconst(m,b,KEEL_F64,UINT64_C(0x4000000000000000),&two));
    CHECK(keel_build_fma(m,b,args[0],two,v,&v)); CHECK(keel_build_return(m,b,&v,1));
    start("float_cast",signature(&f64,1,&i64,1,0),&f64,1,&b,args);
    v=convert(b,KEEL_FPTOSI,KEEL_I64,args[0]); CHECK(keel_build_return(m,b,&v,1));
    start("float_unordered",signature(float_params,2,&i32,1,0),float_params,2,&b,args);
    CHECK(keel_build_fcmp(m,b,KEEL_FUNO,args[0],args[1],&v));
    v=convert(b,KEEL_UEXT,KEEL_I32,v); CHECK(keel_build_return(m,b,&v,1));

    /* C record layout drives BYVAL and SRET classification on each target. */
    keel_layout fields[2], pair;
    CHECK(keel_layout_scalar(m,KEEL_I64,&fields[0])); CHECK(keel_layout_scalar(m,KEEL_F64,&fields[1]));
    CHECK(keel_layout_struct(m,fields,2,&pair));
    uint64_t size, alignment, field_offset;
    CHECK(keel_layout_size(m,pair,&size)); CHECK(keel_layout_align(m,pair,&alignment));
    CHECK(keel_layout_field_offset(m,pair,1,&field_offset));
    if (size!=16 || alignment!=8 || field_offset!=8) return 1;
    const keel_abi_value byval={KEEL_PTR,KEEL_ABI_NONE,KEEL_ABI_BYVAL,pair};
    const keel_abi_value sret={KEEL_PTR,KEEL_ABI_NONE,KEEL_ABI_SRET,pair};
    const keel_abi_value make_params[]={sret,i64,f64};
    keel_signature make_sig=signature(make_params,3,NULL,0,0);
    start("make_pair",make_sig,make_params,3,&b,args);
    store(b,args[1],args[0]); store(b,args[2],offset(b,args[0],8));
    CHECK(keel_build_return(m,b,NULL,0));
    keel_signature pair_sig=signature(&byval,1,&f64,1,0);
    start("sum_pair",pair_sig,&byval,1,&b,args);
    keel_value a=convert(b,KEEL_SITOFP,KEEL_F64,load(b,KEEL_I64,args[0]));
    v=binary(b,KEEL_FADD,a,load(b,KEEL_F64,offset(b,args[0],8)));
    CHECK(keel_build_return(m,b,&v,1));
    keel_symbol host_pair=external("host_pair",pair_sig);
    start("call_host_pair",pair_sig,&byval,1,&b,args);
    keel_call_argument pair_arg={args[0],byval};
    CHECK(keel_build_call_abi(m,b,host_pair,&pair_arg,1,&v,1)); CHECK(keel_build_return(m,b,&v,1));
    keel_symbol host_make=external("host_make_pair",make_sig);
    start("call_host_make_pair",make_sig,make_params,3,&b,args);
    keel_call_argument make_args[3];
    for (size_t i=0;i<3;++i) { make_args[i].value=args[i]; make_args[i].abi=make_params[i]; }
    CHECK(keel_build_call_abi(m,b,host_make,make_args,3,NULL,0)); CHECK(keel_build_return(m,b,NULL,0));
    const keel_abi_value callback_params[]={sret,ptr,i64,f64};
    start("call_pair_callback",signature(callback_params,4,NULL,0,0),callback_params,4,&b,args);
    make_args[0].value=args[0]; make_args[1].value=args[2]; make_args[2].value=args[3];
    CHECK(keel_build_call_indirect_abi(m,b,make_sig,args[1],make_args,3,NULL,0)); CHECK(keel_build_return(m,b,NULL,0));

    keel_signature variadic=signature(&i32,1,&f64,1,KEEL_SIGNATURE_VARIADIC);
    keel_symbol host_var=external("host_variadic",variadic);
    start("call_variadic",signature(NULL,0,&f64,1,0),NULL,0,&b,NULL);
    keel_value varargs[3]={iconst(b,KEEL_I32,7),iconst(b,KEEL_I32,-2),{0,0,0}};
    CHECK(keel_build_fconst(m,b,KEEL_F64,UINT64_C(0x400c000000000000),&varargs[2]));
    CHECK(keel_build_call(m,b,host_var,varargs,3,&v,1)); CHECK(keel_build_return(m,b,&v,1));

    /* A variadic definition traverses register and stack arguments via va_arg. */
    keel_function f=start("sum_varargs",signature(&i32,1,&i64,1,KEEL_SIGNATURE_VARIADIC),&i32,1,&b,args);
    keel_stack_slot va_slot; keel_value list;
    CHECK(keel_stack_slot_create(m,f,32,8,&va_slot)); CHECK(keel_build_stack_address(m,b,va_slot,&list));
    intrinsic(b,"va_start",&list,1);
    keel_block header,body,done;
    CHECK(keel_block_create(m,f,&header)); CHECK(keel_block_create(m,f,&body)); CHECK(keel_block_create(m,f,&done));
    keel_value index,accumulator,answer;
    CHECK(keel_block_add_parameter(m,header,KEEL_I32,&index)); CHECK(keel_block_add_parameter(m,header,KEEL_I64,&accumulator));
    CHECK(keel_block_add_parameter(m,done,KEEL_I64,&answer));
    keel_value initial[2]={iconst(b,KEEL_I32,0),iconst(b,KEEL_I64,0)};
    CHECK(keel_build_jump(m,b,header,initial,2));
    CHECK(keel_build_icmp(m,header,KEEL_SLT,index,args[0],&v));
    CHECK(keel_build_brif(m,header,v,body,NULL,0,done,&accumulator,1));
    CHECK(keel_build_va_arg(m,body,KEEL_I64,list,&v));
    keel_value next[2]={binary(body,KEEL_IADD,index,iconst(body,KEEL_I32,1)),binary(body,KEEL_IADD,accumulator,v)};
    CHECK(keel_build_jump(m,body,header,next,2)); CHECK(keel_build_return(m,done,&answer,1));

    const keel_abi_value atomic_params[]={ptr,i64};
    start("atomic_update",signature(atomic_params,2,&i64,1,0),atomic_params,2,&b,args);
    CHECK(keel_build_atomic_rmw(m,b,KEEL_RMW_ADD,KEEL_SEQ_CST,args[0],args[1],&v));
    CHECK(keel_build_fence(m,b,KEEL_ACQ_REL)); CHECK(keel_build_return(m,b,&v,1));
    const keel_abi_value cas_params[]={ptr,i64,i64};
    start("atomic_compare",signature(cas_params,3,&i32,1,0),cas_params,3,&b,args);
    CHECK(keel_build_atomic_cas(m,b,KEEL_SEQ_CST,KEEL_ACQUIRE,KEEL_CAS_STRONG,args[0],args[1],args[2],results,2));
    v=convert(b,KEEL_UEXT,KEEL_I32,results[1]); CHECK(keel_build_return(m,b,&v,1));
    start("atomic_read",signature(&ptr,1,&i64,1,0),&ptr,1,&b,args);
    CHECK(keel_build_atomic_load(m,b,KEEL_I64,KEEL_ACQUIRE,args[0],&v)); CHECK(keel_build_return(m,b,&v,1));
    start("atomic_write",signature(atomic_params,2,NULL,0,0),atomic_params,2,&b,args);
    CHECK(keel_build_atomic_store(m,b,KEEL_RELEASE,args[1],args[0])); CHECK(keel_build_return(m,b,NULL,0));

    /* Registry memory intrinsics and constraint-parsed assembly. */
    const keel_abi_value copy_params[]={ptr,ptr,i64};
    start("copy_bytes",signature(copy_params,3,NULL,0,0),copy_params,3,&b,args);
    intrinsic(b,"memcpy",args,3); CHECK(keel_build_return(m,b,NULL,0));
    start("asm_identity",signature(&i64,1,&i64,1,0),&i64,1,&b,args);
    const keel_type outputs[]={KEEL_I64};
    CHECK(keel_build_inline_asm(m,b,(const uint8_t *)"",0,(const uint8_t *)"=r,0",4,outputs,1,args,1,&v));
    CHECK(keel_build_return(m,b,&v,1));

    /* Portable vectors avoid target-specific C vector ABI differences. */
    start("vector_sum",signature(&i32,1,&i32,1,0),&i32,1,&b,args);
    keel_type vector_type; CHECK(keel_type_vector(m,KEEL_I32,4,&vector_type));
    CHECK(keel_build_vsplat(m,b,vector_type,args[0],&v));
    keel_value vector; CHECK(keel_build_vector_binary(m,b,(const uint8_t *)"viadd",5,v,v,&vector));
    CHECK(keel_build_vector_reduce(m,b,0,vector,&v)); CHECK(keel_build_return(m,b,&v,1));

    /* Switch is useful for enum tags; each arm supplies its own SSA result. */
    f=start("classify",signature(&i64,1,&i64,1,0),&i64,1,&b,args);
    keel_block first,second,other;
    CHECK(keel_block_create(m,f,&first)); CHECK(keel_block_create(m,f,&second)); CHECK(keel_block_create(m,f,&other));
    const keel_switch_case cases[]={{1,first,NULL,0},{3,second,NULL,0}};
    CHECK(keel_build_switch(m,b,args[0],other,NULL,0,cases,2));
    v=iconst(first,KEEL_I64,10); CHECK(keel_build_return(m,first,&v,1));
    v=iconst(second,KEEL_I64,30); CHECK(keel_build_return(m,second,&v,1));
    v=iconst(other,KEEL_I64,-1); CHECK(keel_build_return(m,other,&v,1));

    /* Global records combine relocations and native-order scalar fields. */
    keel_symbol text_symbol,view_symbol;
    const uint8_t text_bytes[]="hello";
    CHECK(keel_module_add_global(m,(const uint8_t *)"view_text",9,text_bytes,5,1,KEEL_GLOBAL_CONSTANT|KEEL_GLOBAL_LOCAL,&text_symbol));
    keel_data_item view_items[2]; memset(view_items,0,sizeof(view_items));
    view_items[0].kind=KEEL_DATA_ITEM_POINTER; view_items[0].symbol=text_symbol;
    view_items[1].kind=KEEL_DATA_ITEM_INTEGER; view_items[1].type=KEEL_I64; view_items[1].value=5;
    CHECK(keel_module_add_global_items(m,(const uint8_t *)"text_view",9,view_items,2,8,KEEL_GLOBAL_CONSTANT,&view_symbol));

    keel_symbol tls;
    CHECK(keel_module_add_zero_global(m,(const uint8_t *)"tls_counter",11,8,8,0,&tls));
    CHECK(keel_global_set_properties(m,tls,KEEL_DATA_TLS));
    start("tls_increment",signature(NULL,0,&i64,1,0),NULL,0,&b,NULL);
    keel_value tls_address; CHECK(keel_build_symbol_address(m,b,tls,&tls_address));
    v=binary(b,KEEL_IADD,load(b,KEEL_I64,tls_address),iconst(b,KEEL_I64,1));
    store(b,v,tls_address); CHECK(keel_build_return(m,b,&v,1));

    CHECK(keel_symbol_set_function_attributes(m,host_pair,KEEL_FUNCTION_NOINLINE | KEEL_FUNCTION_NOUNWIND,0));
    keel_source_file source;
    CHECK(keel_source_file_add(m,(const uint8_t *)"compiler.tern",13,&source));
    CHECK(keel_instruction_set_source(m,b,0,source,1,1));
    CHECK(keel_module_verify(m));
    if (!strcmp(argv[3],"optimized")) CHECK(keel_module_optimize_with_tier(m,3));
    else if (strcmp(argv[3],"baseline")) return 1;
    keel_codegen_options options={0,0,0,1,0,3};
    keel_buffer *object; CHECK(keel_module_emit_object_with_options(m,target,options,&object));
    keel_module_destroy(m);
    FILE *file=fopen(argv[2],"wb"); if (!file) { keel_buffer_destroy(object); return 1; }
    size_t length=keel_buffer_size(object);
    int written=fwrite(keel_buffer_data(object),1,length,file)==length;
    int closed=fclose(file)==0; keel_buffer_destroy(object); return written&&closed?0:1;
}
