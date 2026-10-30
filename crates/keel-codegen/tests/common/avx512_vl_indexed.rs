//! Independent scalar and system-compiler VSIB contracts for both frontend routes.
use keel_ir::VectorType;
#[derive(Clone, Copy)]
pub struct Shape {
    pub bits: u16,
    pub tag: &'static str,
    pub index: VectorType,
    pub data: VectorType,
    pub lanes: u8,
    pub gather: &'static str,
    pub scatter: &'static str,
    pub api: &'static str,
}
pub const SHAPES: [Shape; 16] = [
    Shape {
        bits: 128,
        tag: "dd",
        index: VectorType::I32x4,
        data: VectorType::I32x4,
        lanes: 4,
        gather: "vpgatherdd",
        scatter: "vpscatterdd",
        api: "i32gather_epi32",
    },
    Shape {
        bits: 128,
        tag: "dps",
        index: VectorType::I32x4,
        data: VectorType::F32x4,
        lanes: 4,
        gather: "vgatherdps",
        scatter: "vscatterdps",
        api: "i32gather_ps",
    },
    Shape {
        bits: 128,
        tag: "dq",
        index: VectorType::I32x4,
        data: VectorType::I64x2,
        lanes: 2,
        gather: "vpgatherdq",
        scatter: "vpscatterdq",
        api: "i32gather_epi64",
    },
    Shape {
        bits: 128,
        tag: "dpd",
        index: VectorType::I32x4,
        data: VectorType::F64x2,
        lanes: 2,
        gather: "vgatherdpd",
        scatter: "vscatterdpd",
        api: "i32gather_pd",
    },
    Shape {
        bits: 128,
        tag: "qd",
        index: VectorType::I64x2,
        data: VectorType::I32x4,
        lanes: 2,
        gather: "vpgatherqd",
        scatter: "vpscatterqd",
        api: "i64gather_epi32",
    },
    Shape {
        bits: 128,
        tag: "qps",
        index: VectorType::I64x2,
        data: VectorType::F32x4,
        lanes: 2,
        gather: "vgatherqps",
        scatter: "vscatterqps",
        api: "i64gather_ps",
    },
    Shape {
        bits: 128,
        tag: "qq",
        index: VectorType::I64x2,
        data: VectorType::I64x2,
        lanes: 2,
        gather: "vpgatherqq",
        scatter: "vpscatterqq",
        api: "i64gather_epi64",
    },
    Shape {
        bits: 128,
        tag: "qpd",
        index: VectorType::I64x2,
        data: VectorType::F64x2,
        lanes: 2,
        gather: "vgatherqpd",
        scatter: "vscatterqpd",
        api: "i64gather_pd",
    },
    Shape {
        bits: 256,
        tag: "dd",
        index: VectorType::I32x8,
        data: VectorType::I32x8,
        lanes: 8,
        gather: "vpgatherdd",
        scatter: "vpscatterdd",
        api: "i32gather_epi32",
    },
    Shape {
        bits: 256,
        tag: "dps",
        index: VectorType::I32x8,
        data: VectorType::F32x8,
        lanes: 8,
        gather: "vgatherdps",
        scatter: "vscatterdps",
        api: "i32gather_ps",
    },
    Shape {
        bits: 256,
        tag: "dq",
        index: VectorType::I32x4,
        data: VectorType::I64x4,
        lanes: 4,
        gather: "vpgatherdq",
        scatter: "vpscatterdq",
        api: "i32gather_epi64",
    },
    Shape {
        bits: 256,
        tag: "dpd",
        index: VectorType::I32x4,
        data: VectorType::F64x4,
        lanes: 4,
        gather: "vgatherdpd",
        scatter: "vscatterdpd",
        api: "i32gather_pd",
    },
    Shape {
        bits: 256,
        tag: "qd",
        index: VectorType::I64x4,
        data: VectorType::I32x4,
        lanes: 4,
        gather: "vpgatherqd",
        scatter: "vpscatterqd",
        api: "i64gather_epi32",
    },
    Shape {
        bits: 256,
        tag: "qps",
        index: VectorType::I64x4,
        data: VectorType::F32x4,
        lanes: 4,
        gather: "vgatherqps",
        scatter: "vscatterqps",
        api: "i64gather_ps",
    },
    Shape {
        bits: 256,
        tag: "qq",
        index: VectorType::I64x4,
        data: VectorType::I64x4,
        lanes: 4,
        gather: "vpgatherqq",
        scatter: "vpscatterqq",
        api: "i64gather_epi64",
    },
    Shape {
        bits: 256,
        tag: "qpd",
        index: VectorType::I64x4,
        data: VectorType::F64x4,
        lanes: 4,
        gather: "vgatherqpd",
        scatter: "vscatterqpd",
        api: "i64gather_pd",
    },
];
pub fn harness() -> String {
    let mut c = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <sys/prctl.h>
#include <unistd.h>
#include <signal.h>
typedef void(*function)(void*,void*,void*,void*,uint8_t);
struct testcase{function gather,scatter,all,reference_gather,reference_scatter,reference_all;unsigned bits,ibits,dbits,ibytes,dbytes,lanes,scale,alias;const char*name;};
"#,
    );
    let mut cases = String::new();
    for shape in SHAPES {
        let width = shape.bits;
        let tag = shape.tag;
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let ibits = shape.index.lane_type().bytes().unwrap() * 8;
        let dbits = shape.data.lane_type().bytes().unwrap() * 8;
        let ibytes = shape.index.bytes();
        let dbytes = shape.data.bytes();
        let index = format!("__m{}i", ibytes * 8);
        let data = if shape.data.lane_type().is_int() {
            format!("__m{}i", dbytes * 8)
        } else {
            format!("__m{}{}", dbytes * 8, if dbits == 64 { "d" } else { "" })
        };
        let scatter_api = shape.api.replace("gather", "scatter");
        for scale in [1, 2, 4, 8] {
            for alias in [false, true] {
                let suffix = format!("{tag}{width}_s{scale}_a{}", u8::from(alias));
                let g = format!("gather_{suffix}");
                let sc = format!("scatter_{suffix}");
                let all = format!("scatterall_{suffix}");
                let active = shape.lanes;
                for name in [&g, &sc, &all] {
                    c += &format!("extern void {name}(void*,void*,void*,void*,uint8_t);\n");
                }
                c += &format!(
                    "static void reference_{g}(void*base,void*ip,void*dp,void*out,uint8_t k){{{index} i;{data} d;memcpy(&i,ip,{ibytes});memcpy(&d,dp,{dbytes});d={prefix}_mmask_{}(d,k,i,base,{scale});memcpy(out,&d,{dbytes});}}\n",
                    shape.api
                );
                for (name, masked) in [(&sc, true), (&all, false)] {
                    let call = if masked {
                        format!("{prefix}_mask_{scatter_api}(base,k,i,d,{scale})")
                    } else {
                        format!("{prefix}_{scatter_api}(base,i,d,{scale})")
                    };
                    c += &format!(
                        "static void reference_{name}(void*base,void*ip,void*dp,void*out,uint8_t k){{{index} i;{data} d;memcpy(&i,ip,{ibytes});memcpy(&d,dp,{dbytes});{call};}}\n"
                    );
                }
                cases += &format!(
                    "{{{g},{sc},{all},reference_{g},reference_{sc},reference_{all},{width},{ibits},{dbits},{ibytes},{dbytes},{active},{scale},{},\"{suffix}\"}},\n",
                    u8::from(alias)
                );
            }
        }
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
static int64_t get_index(void*p,unsigned bits,unsigned lane){if(bits==32){int32_t x;memcpy(&x,(char*)p+lane*4,4);return x;}int64_t x;memcpy(&x,(char*)p+lane*8,8);return x;}
static void set_index(void*p,unsigned bits,unsigned lane,int64_t x){memcpy((char*)p+lane*(bits/8),&x,bits/8);}
static int preserved(struct testcase t,void*out,void*index,void*data,uint8_t k){return memcmp((char*)out+64,index,t.ibytes)||memcmp((char*)out+128,data,t.dbytes)||*((unsigned char*)out+192)!=k||(t.alias&&memcmp((char*)out+256,data,64));}
int main(void){unsigned saved=_mm_getcsr();if(prctl(PR_SET_DUMPABLE,0))return 1;long page=sysconf(_SC_PAGESIZE);unsigned char*guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(guard==MAP_FAILED||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 2;unsigned char*memory=guard+page,*original=malloc(page),*expected=malloc(page),*reference=malloc(page);if(!original||!expected||!reference)return 3;
uint64_t values[]={UINT64_C(0x7ff012347f812345),UINT64_C(0xfff85432ffc54321),UINT64_C(0x8000000080000000),UINT64_C(0x0000000100000001),UINT64_MAX,0,UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x3ff000003f800000)};
for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase t=cases[ci];uint64_t index[8] __attribute__((aligned(64))),data[8] __attribute__((aligned(64)));unsigned char out[336],want[64],system[64];unsigned lane_bytes=t.dbits/8,index_lanes=t.ibytes/(t.ibits/8);uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<t.lanes)-1),(uint8_t)(1u<<(t.lanes-1))};for(unsigned mi=0;mi<8;mi++){uint8_t k=masks[mi],valid=k&((1u<<t.lanes)-1);unsigned raw=k;for(unsigned j=0;j<8;j++){index[j]=0;data[j]=values[(j+raw)%8];}for(unsigned lane=0;lane<index_lanes;lane++){int64_t offset;if(!valid)offset=t.ibits==64?INT64_C(0x1000000000000000):0;else if(lane<t.lanes&&(k&(1u<<lane)))offset=(int64_t)lane*32*(lane&1?-1:1);else offset=page/2/t.scale;set_index(index,t.ibits,lane,offset);}if(t.alias&&t.ibytes==t.dbytes)memcpy(data,index,t.ibytes);
for(long j=0;j<page;j++)original[j]=(unsigned char)(j*37+11);for(unsigned lane=0;lane<t.lanes;lane++){int64_t offset=(int64_t)lane*32*(lane&1?-1:1);uint64_t value=values[lane%8];memcpy(original+page/2+offset*t.scale,&value,lane_bytes);}memcpy(memory,original,page);void*base=valid?(void*)(memory+page/2):(void*)(guard+2*page);unsigned csr=(saved&~0xe0c0u)|0x3f|((raw&128)?0xe040u:0);_mm_setcsr(csr);memset(out,0xa5,sizeof out);t.reference_gather(base,index,data,system,k);t.gather(base,index,data,out,k);if(_mm_getcsr()!=csr||memcmp(out,system,t.dbytes)){fprintf(stderr,"%s mask=%x gather system mismatch\n",t.name,k);return 4;}memcpy(want,data,t.dbytes);if(t.dbytes>t.lanes*lane_bytes)memset(want+t.lanes*lane_bytes,0,t.dbytes-t.lanes*lane_bytes);for(unsigned lane=0;lane<t.lanes;lane++)if(k&(1u<<lane))memcpy(want+lane*lane_bytes,(char*)base+get_index(index,t.ibits,lane)*t.scale,lane_bytes);if(memcmp(out,want,t.dbytes)||preserved(t,out,index,data,k))return 5;for(unsigned j=t.dbytes;j<64;j++)if(out[j]!=0xa5)return 6;
memcpy(memory,original,page);memcpy(expected,original,page);memcpy(reference,original,page);for(unsigned lane=0;lane<t.lanes;lane++)if(k&(1u<<lane))memcpy(expected+page/2+get_index(index,t.ibits,lane)*t.scale,(char*)data+lane*lane_bytes,lane_bytes);void*reference_base=valid?(void*)(reference+page/2):(void*)(guard+2*page);memset(out,0xa5,sizeof out);t.reference_scatter(reference_base,index,data,system,k);t.scatter(base,index,data,out,k);if(_mm_getcsr()!=csr||memcmp(memory,expected,page)||memcmp(reference,expected,page)||memcmp(out,data,t.dbytes)||preserved(t,out,index,data,k)){fprintf(stderr,"%s mask=%x scatter mismatch\n",t.name,k);return 7;}
if(raw==255){memcpy(memory,original,page);memcpy(reference,original,page);t.reference_all(reference+page/2,index,data,system,k);t.all(memory+page/2,index,data,out,k);if(_mm_getcsr()!=csr||memcmp(memory,expected,page)||memcmp(reference,expected,page)||preserved(t,out,index,data,k))return 8;}
}
if(!t.alias&&t.scale==8){for(unsigned scatter=0;scatter<2;scatter++)for(unsigned high=0;high<(t.ibits==64?2u:1u);high++){set_index(index,t.ibits,0,high?INT64_C(0x1000000000000000):page/2/t.scale);pid_t child=fork();if(child<0)return 9;if(!child){if(scatter)t.scatter(memory+page/2,index,data,out,1);else t.gather(memory+page/2,index,data,out,1);_exit(99);}int status;if(waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 10;}}
}
_mm_setcsr(saved);free(original);free(expected);free(reference);munmap(guard,page*3);return 0;}
"#;
    c
}
