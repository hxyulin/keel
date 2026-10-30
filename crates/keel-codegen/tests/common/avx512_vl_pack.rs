//! Shared scalar and system-compiler oracles for the two Keel frontend routes.
use keel_ir::VectorType;
pub const SHAPES: [(u32, &str, VectorType, &str, &str); 8] = [
    (128, "d", VectorType::I32x4, "epi32", "__m128i"),
    (128, "q", VectorType::I64x2, "epi64", "__m128i"),
    (128, "ps", VectorType::F32x4, "ps", "__m128"),
    (128, "pd", VectorType::F64x2, "pd", "__m128d"),
    (256, "d", VectorType::I32x8, "epi32", "__m256i"),
    (256, "q", VectorType::I64x4, "epi64", "__m256i"),
    (256, "ps", VectorType::F32x8, "ps", "__m256"),
    (256, "pd", VectorType::F64x4, "pd", "__m256d"),
];
pub fn harness() -> String {
    let mut c = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <sys/prctl.h>
#include <unistd.h>
#include <signal.h>
typedef void(*function)(void*,void*,void*,uint8_t);
struct testcase{function run,reference;unsigned bytes,lane_bytes,operation,mode;const char*name;};
"#,
    );
    let mut cases = String::new();
    let mut memory = String::new();
    for (width, suffix, shape, api, vector) in SHAPES {
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let bytes = width / 8;
        let lane_bytes = shape.lane_type().bytes().unwrap();
        let lanes = shape.lanes();
        for (operation, op) in ["compress", "expand"].into_iter().enumerate() {
            for mode in 0..3 {
                let name = format!("{op}_{suffix}{width}_{mode}");
                let input = if mode == 1 { "m" } else { "a" };
                let expression = if mode == 2 {
                    format!("{prefix}_maskz_{op}_{api}(k,{input})")
                } else {
                    format!("{prefix}_mask_{op}_{api}(m,k,{input})")
                };
                c += &format!(
                    "extern void {name}(void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*mp,void*ap,void*out,uint8_t k){{{vector} m,a;memcpy(&m,mp,{bytes});memcpy(&a,ap,{bytes});{vector} r={expression};memcpy(out,&r,{bytes});}}\n"
                );
                cases += &format!(
                    "{{{name},reference_{name},{bytes},{lane_bytes},{operation},{mode},\"{name}\"}},\n"
                );
            }
        }
        c += &format!(
            "extern void compressstore_{suffix}{width}(void*,void*,void*,uint8_t),expandload_{suffix}{width}_0(void*,void*,void*,uint8_t),expandload_{suffix}{width}_1(void*,void*,void*,uint8_t);\n"
        );
        memory += &format!(
            r#"
{{uint64_t m[8],a[8];unsigned char out[336],want[64],reference[64],original[64];for(unsigned i=0;i<8;i++){{m[i]=values[(i+3)%8];a[i]=values[i];}}memcpy(original,a,64);
for(unsigned raw=0;raw<256;raw++){{uint8_t k=(uint8_t)raw;unsigned count=0;for(unsigned lane=0;lane<{lanes};lane++)count+=(k>>lane)&1;
for(unsigned unaligned=0;unaligned<2;unaligned++){{memset(guard+page,0xa5,page);unsigned char*packed=guard+2*page-count*{lane_bytes}-unaligned;void*address=count?(void*)packed:(void*)(guard+2*page);unsigned csr=(saved&~0xe0c0u)|0x3f|((raw&128)?0xe040u:0);_mm_setcsr(csr);memset(out,0xa5,sizeof out);compressstore_{suffix}{width}(a,address,out,k);if(_mm_getcsr()!=csr||memcmp(out,a,{bytes})||out[64]!=k)return 10;
unsigned position=0;for(unsigned lane=0;lane<{lanes};lane++)if((k>>lane)&1){{memcpy(want+position*{lane_bytes},(char*)a+lane*{lane_bytes},{lane_bytes});position++;}}if(memcmp(packed,want,count*{lane_bytes}))return 11;if(guard[2*page-count*{lane_bytes}-unaligned-1]!=0xa5||(unaligned&&guard[2*page-1]!=0xa5))return 12;
{vector} source;memcpy(&source,a,{bytes});memset(reference,0xa5,64);{prefix}_mask_compressstoreu_{api}(reference,k,source);if(memcmp(reference,want,count*{lane_bytes}))return 13;
for(unsigned zero=0;zero<2;zero++){{memcpy(want,m,{bytes});if(zero)memset(want,0,{bytes});position=0;for(unsigned lane=0;lane<{lanes};lane++)if((k>>lane)&1)memcpy(want+lane*{lane_bytes},packed+position++*{lane_bytes},{lane_bytes});memset(out,0xa5,sizeof out);_mm_setcsr(csr);if(zero)expandload_{suffix}{width}_1(m,address,out,k);else expandload_{suffix}{width}_0(m,address,out,k);if(_mm_getcsr()!=csr||memcmp(out,want,{bytes})||memcmp(out+64,m,{bytes})||out[128]!=k)return 14;{vector} merge,result;memcpy(&merge,m,{bytes});if(zero)result={prefix}_maskz_expandloadu_{api}(k,address);else result={prefix}_mask_expandloadu_{api}(merge,k,address);memcpy(reference,&result,{bytes});if(_mm_getcsr()!=csr||memcmp(reference,want,{bytes}))return 15;for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 16;}}
}}
}}
/* Packed source and destination may overlap the original vector memory. */
for(unsigned raw=0;raw<256;raw+=17){{uint8_t k=(uint8_t)raw;memcpy(a,original,64);unsigned position=0;for(unsigned lane=0;lane<{lanes};lane++)if((k>>lane)&1)memcpy(want+position++*{lane_bytes},original+lane*{lane_bytes},{lane_bytes});compressstore_{suffix}{width}(a,a,out,k);if(memcmp(a,want,position*{lane_bytes})||memcmp(out,original,{bytes}))return 17;memcpy(m,original,64);memcpy(want,m,{bytes});position=0;for(unsigned lane=0;lane<{lanes};lane++)if((k>>lane)&1)memcpy(want+lane*{lane_bytes},original+position++*{lane_bytes},{lane_bytes});expandload_{suffix}{width}_0(m,m,out,k);if(memcmp(out,want,{bytes})||memcmp(out+64,original,{bytes}))return 18;}}
/* Two active lanes must fault if only one packed element is accessible. */
for(unsigned store=0;store<2;store++){{pid_t child=fork();if(child<0)return 19;if(!child){{if(store)compressstore_{suffix}{width}(a,guard+2*page-{lane_bytes},out,3);else expandload_{suffix}{width}_0(m,guard+2*page-{lane_bytes},out,3);_exit(99);}}int status;if(waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 20;}}
}}
"#
        );
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
int main(void){unsigned saved=_mm_getcsr();if(prctl(PR_SET_DUMPABLE,0))return 1;long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;
uint64_t values[]={0,UINT64_MAX,UINT64_C(0x800000007f812345),UINT64_C(0x7ff012347fc54321),UINT64_C(0xfff05432ffc12345),UINT64_C(0x0000000100000001),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x8000000000000000)};
for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase t=cases[ci];unsigned lanes=t.bytes/t.lane_bytes;uint64_t m[8],a[8];unsigned char out[336],want[64],reference[64];for(unsigned sample=0;sample<8;sample++){for(unsigned i=0;i<8;i++){m[i]=values[(i+sample+3)%8];a[i]=values[(i+sample)%8];}for(unsigned raw=0;raw<256;raw++){uint8_t k=(uint8_t)raw;unsigned csr=(saved&~0xe0c0u)|0x3f|((sample&1)?0xe040u:0);_mm_setcsr(csr);memset(out,0xa5,sizeof out);t.reference(m,a,reference,k);t.run(m,a,out,k);if(_mm_getcsr()!=csr||memcmp(out,reference,t.bytes)){fprintf(stderr,"%s sample=%u mask=%x system mismatch\n",t.name,sample,k);return 3;}memcpy(want,m,t.bytes);if(t.mode==2)memset(want,0,t.bytes);void*input=t.mode==1?(void*)m:(void*)a;unsigned position=0;for(unsigned lane=0;lane<lanes;lane++)if((k>>lane)&1){if(t.operation==0)memcpy(want+position++*t.lane_bytes,(char*)input+lane*t.lane_bytes,t.lane_bytes);else memcpy(want+lane*t.lane_bytes,(char*)input+position++*t.lane_bytes,t.lane_bytes);}if(memcmp(out,want,t.bytes)||memcmp(out+64,m,t.bytes)||memcmp(out+128,a,t.bytes)||out[192]!=k||(t.mode==1&&memcmp(out+256,m,64)))return 4;for(unsigned i=t.bytes;i<64;i++)if(out[i]!=0xa5)return 5;}}}
"#;
    c += &memory;
    c += "_mm_setcsr(saved);munmap(guard,page*3);return 0;}\n";
    c
}
