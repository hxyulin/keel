#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
@DECLS@
@REFERENCES@
struct Case{const char*name;void(*run)(const void*,const void*,const void*,void*,uint64_t);void(*reference)(const void*,const void*,const void*,void*,uint64_t);unsigned bytes,input_bits,result_bits,operation,alias;};
static const struct Case cases[]={
@TABLE@
};
static unsigned get(const unsigned char*p,unsigned bits){unsigned x=0;memcpy(&x,p,bits/8);return x;}
static void put(unsigned char*p,unsigned bits,unsigned x){memcpy(p,&x,bits/8);}
static unsigned scalar(const struct Case*t,const unsigned char*a,const unsigned char*b,unsigned i){
 unsigned output_block=128/t->result_bits,block=i/output_block,position=i%output_block;
 if(t->operation<4){
  unsigned input_block=128/t->input_bits,index=block*input_block+position%input_block;
  unsigned raw=get((position<input_block?a:b)+index*t->input_bits/8,t->input_bits);
  int64_t value=(raw&(UINT64_C(1)<<(t->input_bits-1)))?(int64_t)raw-(INT64_C(1)<<t->input_bits):raw;
  int is_unsigned=t->operation==1||t->operation==3;
  int64_t minimum=is_unsigned?0:-(INT64_C(1)<<(t->result_bits-1));
  int64_t maximum=(INT64_C(1)<<(t->result_bits-(is_unsigned?0:1)))-1;
  int64_t clipped=value<minimum?minimum:value>maximum?maximum:value;
  return (uint32_t)clipped&((1u<<t->result_bits)-1);
 }
 unsigned high=t->operation==5||t->operation==7;
 unsigned index=block*output_block+(high?output_block/2:0)+position/2;
 return get(((position&1)?b:a)+index*t->input_bits/8,t->input_bits);
}
int main(void){
 unsigned byte_edges[]={0,1,127,128,129,255,254,42,84,85,170,196,13,29,63,96};
 unsigned word_edges[]={0,1,127,128,129,255,256,257,0x7fff,0x8000,0xffff,0xff7f,0xff80,0xff81,0x5555,0xaaaa};
 unsigned dword_edges[]={0,1,32767,32768,32769,65535,65536,65537,0x7fffffff,0x80000000,0xffffffff,0xffff7fff,0xffff8000,0xffff8001,0x12345678,0x87654321};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;unsigned n=t->bytes*8/t->result_bits,input_n=t->bytes*8/t->input_bits;
  uint64_t all=n==64?UINT64_MAX:(UINT64_C(1)<<n)-1,boundaries=0;unsigned block=128/t->result_bits;
  for(unsigned bit=block;bit<n;bit+=block)boundaries|=(UINT64_C(1)<<(bit-1))|(UINT64_C(1)<<bit);
  uint64_t masks[]={0,all,UINT64_C(0xa55a8001c33c8001),1,UINT64_C(1)<<(n-1),~all,boundaries};
  for(unsigned sample=0;sample<16;sample++){
   unsigned char m[64],a[64],b[64],out[272],reference[64];
   for(unsigned i=0;i<n;i++)put(m+i*t->result_bits/8,t->result_bits,0x12345678u+sample*17+i*53);
   for(unsigned i=0;i<input_n;i++){
    const unsigned *edges=t->input_bits==8?byte_edges:t->input_bits==16?word_edges:dword_edges;
    unsigned input_block=(i*t->input_bits/8)/16;
    unsigned x=edges[(sample+i+input_block*3)%16],y=edges[(sample*5+i*7+input_block*5+1)%16];
    put(a+i*t->input_bits/8,t->input_bits,x);put(b+i*t->input_bits/8,t->input_bits,y);
   }
   for(unsigned p=0;p<7;p++){
    uint64_t k=masks[p],saved=0;memset(out,0xa5,sizeof out);t->run(m,a,b,out,k);t->reference(m,a,b,reference,k);
    if(memcmp(out,reference,t->bytes)){printf("native %s sample%u mask%llx\n",t->name,sample,(unsigned long long)k);return 1;}
    for(unsigned i=0;i<n;i++){
     unsigned merge=get((t->alias==2?a:m)+i*t->result_bits/8,t->result_bits);
     unsigned expected=((k>>i)&1)?scalar(t,a,t->alias==1?a:b,i):merge;
     if(get(out+i*t->result_bits/8,t->result_bits)!=expected){printf("scalar %s sample%u lane%u got%x expected%x\n",t->name,sample,i,get(out+i*t->result_bits/8,t->result_bits),expected);return 2;}
    }
    memcpy(&saved,out+256,8);if(memcmp(m,out+64,t->bytes)||memcmp(a,out+128,t->bytes)||memcmp(b,out+192,t->bytes)||saved!=k)return 3;
    if((t->bytes<64&&out[t->bytes]!=0xa5)||out[264]!=0xa5)return 4;
   }
  }
 }
 return 0;
}
