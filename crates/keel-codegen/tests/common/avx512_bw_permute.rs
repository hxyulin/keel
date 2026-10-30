//! Explicit lane-local and full-vector BW permutation contracts for both frontends.
use keel_ir::{Type, VectorType};
#[derive(Clone, Copy)]
pub struct Case {
    pub width: u16,
    pub vector: VectorType,
    pub mask: Type,
    pub operation: u8,
    pub stem: &'static str,
    pub api: &'static str,
    pub control: u8,
    pub mode: u8,
}
impl Case {
    pub fn name(self) -> String {
        format!(
            "{}_{}_{}_{}",
            self.stem, self.width, self.control, self.mode
        )
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (width, byte, word, byte_mask, word_mask) in [
        (
            128,
            VectorType::I8x16,
            VectorType::I16x8,
            Type::I16,
            Type::I8,
        ),
        (
            256,
            VectorType::I8x32,
            VectorType::I16x16,
            Type::I32,
            Type::I16,
        ),
        (
            512,
            VectorType::I8x64,
            VectorType::I16x32,
            Type::I64,
            Type::I32,
        ),
    ] {
        for (operation, stem, api) in [
            (0, "pshufb", "shuffle_epi8"),
            (1, "palignr", "alignr_epi8"),
            (2, "pshufhw", "shufflehi_epi16"),
            (3, "pshuflw", "shufflelo_epi16"),
            (4, "permw", "permutexvar_epi16"),
            (5, "permt2w", "permutex2var_epi16"),
        ] {
            let controls: &[u8] = match operation {
                1 => &[0, 1, 15, 16, 17, 31, 32, 255],
                2 | 3 => &[0, 0x55, 0xaa, 0xff, 0x1b, 0x4e, 0xb1, 0xe4],
                _ => &[0],
            };
            let modes = if operation == 5 {
                5
            } else if operation == 4 || width == 512 {
                4
            } else {
                3
            };
            for &control in controls {
                for mode in 0..modes {
                    cases.push(Case {
                        width,
                        vector: if operation < 2 { byte } else { word },
                        mask: if operation < 2 { byte_mask } else { word_mask },
                        operation,
                        stem,
                        api,
                        control,
                        mode,
                    });
                }
            }
        }
    }
    for (operation, stem, api) in [(6, "pslldq", "bslli_epi128"), (7, "psrldq", "bsrli_epi128")] {
        for control in [0, 1, 15, 16, 17, 255] {
            cases.push(Case {
                width: 512,
                vector: VectorType::I8x64,
                mask: Type::I64,
                operation,
                stem,
                api,
                control,
                mode: 3,
            });
        }
    }
    cases
}
pub fn harness() -> String {
    let mut c = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
typedef void(*function)(void*,void*,void*,void*,void*,unsigned long long);
struct testcase{function run,reference;unsigned width,bits,operation,control,mode;const char*name;};
"#,
    );
    let mut entries = String::new();
    for case in cases() {
        let name = case.name();
        let width = case.width;
        let prefix = if width == 128 {
            "_mm"
        } else if width == 256 {
            "_mm256"
        } else {
            "_mm512"
        };
        let bytes = width / 8;
        let bits = case.vector.lane_type().bytes().unwrap() * 8;
        let operation = case.operation;
        let mode = case.mode;
        let input = if mode == 1 { "a" } else { "i" };
        let control = case.control;
        let api = case.api;
        let expression = match operation {
            0 => format!("{prefix}_{api}(a,{input})"),
            1 => format!("{prefix}_{api}(a,{input},{control})"),
            2 | 3 | 6 | 7 => format!("{prefix}_{api}(a,{control})"),
            4 => format!("{prefix}_{api}({input},a)"),
            5 => format!("{prefix}_{api}(a,{input},b)"),
            _ => unreachable!("BW permutation reference recipe"),
        };
        c += &format!(
            "extern void {name}(void*,void*,void*,void*,void*,unsigned long long);\nstatic void reference_{name}(void*mp,void*ap,void*ip,void*bp,void*out,unsigned long long k){{__m{width}i a,i,b,r;memcpy(&a,ap,{bytes});memcpy(&i,ip,{bytes});memcpy(&b,bp,{bytes});r={expression};memcpy(out,&r,{bytes});"
        );
        if mode != 3 {
            let merge = match mode {
                0 => "mp",
                1 => "ap",
                4 => "ip",
                _ => "0",
            };
            if mode == 2 {
                c += &format!(
                    "for(unsigned lane=0;lane<{};lane++)if(!(k&(1ULL<<lane)))memset((char*)out+lane*({bits}/8),0,{bits}/8);",
                    case.vector.lanes()
                );
            } else {
                c += &format!(
                    "for(unsigned lane=0;lane<{};lane++)if(!(k&(1ULL<<lane)))memcpy((char*)out+lane*({bits}/8),(char*){merge}+lane*({bits}/8),{bits}/8);",
                    case.vector.lanes()
                );
            }
        }
        c += "}\n";
        entries += &format!(
            "{{{name},reference_{name},{width},{bits},{operation},{control},{mode},\"{name}\"}},\n"
        );
    }
    c += &format!("static struct testcase cases[]={{\n{entries}}};\n");
    c += r#"
static uint64_t element(void*p,unsigned bits,unsigned lane){uint64_t x=0;memcpy(&x,(char*)p+lane*(bits/8),bits/8);return x;}
static uint64_t scalar(struct testcase t,void*a,void*i,void*b,unsigned lane){unsigned n=t.width/t.bits,local=128/t.bits,group=(lane/local)*local,j=lane%local;uint64_t index=element(i,t.bits,lane);switch(t.operation){case 0:return index&128?0:element(a,8,group+(index&15));case 1:{unsigned p=t.control+j;return p>=32?0:p<16?element(i,8,group+p):element(a,8,group+p-16);}case 2:return element(a,16,group+(j<4?j:4+((t.control>>(2*(j-4)))&3)));case 3:return element(a,16,group+(j>=4?j:((t.control>>(2*j))&3)));case 4:return element(a,16,index&(n-1));case 5:{unsigned p=index&(2*n-1);return element(p<n?a:b,16,p<n?p:p-n);}case 6:return j<t.control?0:element(a,8,group+j-t.control);case 7:return j+t.control>=16?0:element(a,8,group+j+t.control);default:return 0;}}
int main(void){unsigned saved=_mm_getcsr();for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase t=cases[ci];unsigned bytes=t.width/8,lanes=t.width/t.bits;unsigned char m[64],a[64],index[64],b[64],out[464],want[64];uint64_t valid=lanes==64?UINT64_MAX:(1ULL<<lanes)-1;unsigned long long masks[]={0,1,1ULL<<(lanes-1),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0xaaaaaaaa55555555),UINT64_MAX,~valid};unsigned samples[]={0,1,2,3};if(t.operation==0){samples[1]=15;samples[2]=16;samples[3]=17;}else if(t.operation==4||t.operation==5){samples[1]=lanes-1;samples[2]=lanes;samples[3]=2*lanes-1;}for(unsigned sample_index=0;sample_index<4;sample_index++){unsigned sample=samples[sample_index];for(unsigned lane=0;lane<64/(t.bits/8);lane++){uint64_t mv=0xf123u+lane*17u+sample,av=0x1101u+lane*37u+sample*13u,bv=0x8103u+lane*53u+sample*11u,iv;if(t.operation==0){iv=(lane+(sample<16?sample:3))&15;if(sample==16)iv|=0x70;if(sample==17)iv|=0x80|(lane&1?0x70:0);}else if(t.bits==8)iv=lane*19u+sample*3u+0x45;else{iv=(lane+sample)%(2*lanes);if(sample&1)iv|=(uint16_t)~(2*lanes-1);}memcpy(m+lane*(t.bits/8),&mv,t.bits/8);memcpy(a+lane*(t.bits/8),&av,t.bits/8);memcpy(b+lane*(t.bits/8),&bv,t.bits/8);memcpy(index+lane*(t.bits/8),&iv,t.bits/8);}for(unsigned mi=0;mi<7;mi++){uint64_t k=masks[mi];unsigned csr=(saved&~0xe0c0u)|0x3f|((sample&1)?0xe040u:0);_mm_setcsr(csr);memset(out,0xa5,sizeof out);t.reference(m,a,index,b,want,k);t.run(m,a,index,b,out,k);if(_mm_getcsr()!=csr||memcmp(out,want,bytes)){fprintf(stderr,"%s sample=%u mask=%llx system mismatch\n",t.name,sample,(unsigned long long)k);return 1;}for(unsigned lane=0;lane<lanes;lane++){uint64_t merge=t.mode==2?0:element(t.mode==1?(void*)a:t.mode==4?(void*)index:(void*)m,t.bits,lane);uint64_t expected=(t.mode==3||(k&(1ULL<<lane)))?scalar(t,a,t.mode==1?(void*)a:(void*)index,b,lane):merge;if(element(out,t.bits,lane)!=expected){fprintf(stderr,"%s sample=%u lane=%u scalar mismatch\n",t.name,sample,lane);return 2;}}uint64_t after;memcpy(&after,out+320,8);if(memcmp(out+64,m,bytes)||memcmp(out+128,a,bytes)||memcmp(out+192,index,bytes)||memcmp(out+256,b,bytes)||after!=k||(t.mode==1&&memcmp(out+384,m,64)))return 3;for(unsigned j=bytes;j<64;j++)if(out[j]!=0xa5)return 4;}}}_mm_setcsr(saved);return 0;}
"#;
    c
}
