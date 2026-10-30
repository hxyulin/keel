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
static int signed_word(unsigned x){return x&0x8000?(int)x-65536:(int)x;}
static int signed_byte(unsigned x){return x&0x80?(int)x-256:(int)x;}
static unsigned scalar(unsigned operation,const unsigned char*a,const unsigned char*b,unsigned i){
 unsigned x=0,y=0;int sx=0,sy=0;
 if(operation!=6){x=get(a+i*2,16);y=get(b+i*2,16);sx=signed_word(x);sy=signed_word(y);}
 switch(operation){
  case 0:return (x*y)&0xffff;
  case 1:return (uint32_t)(sx*sy)>>16;
  case 2:return (x*y)>>16;
  case 3:{int64_t product=(int64_t)sx*sy+16384;int64_t rounded=product>=0?product/32768:-((-product+32767)/32768);return (uint32_t)rounded&0xffff;}
  case 4:{int64_t sum=(int64_t)signed_word(get(a+i*4,16))*signed_word(get(b+i*4,16))+(int64_t)signed_word(get(a+i*4+2,16))*signed_word(get(b+i*4+2,16));return (uint32_t)sum;}
  case 5:{int sum=a[i*2]*signed_byte(b[i*2])+a[i*2+1]*signed_byte(b[i*2+1]);return (unsigned)(sum<-32768?-32768:sum>32767?32767:sum)&0xffff;}
  case 6:return ((unsigned)a[i]+b[i]+1)>>1;
  case 7:return (x+y+1)>>1;
 }
 return 0;
}
int main(void){
 unsigned words[]={0,1,2,0x7fff,0x8000,0x8001,0xffff,0xfffe,0x4000,0xc000,0x5555,0xaaaa};
 unsigned bytes[]={0,1,2,0x7f,0x80,0x81,0xff,0xfe,0x40,0xc0,0x55,0xaa};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;unsigned n=t->bytes*8/t->result_bits,input_n=t->bytes*8/t->input_bits;
  uint64_t all=n==64?UINT64_MAX:(UINT64_C(1)<<n)-1,boundaries=0;unsigned block=128/t->result_bits;
  for(unsigned bit=block;bit<n;bit+=block)boundaries|=(UINT64_C(1)<<(bit-1))|(UINT64_C(1)<<bit);
  uint64_t masks[]={0,all,UINT64_C(0xa55a8001c33c8001),1,UINT64_C(1)<<(n-1),~all,boundaries};
  for(unsigned sample=0;sample<16;sample++){
   unsigned char m[64],a[64],b[64],out[272],reference[64];
   for(unsigned i=0;i<n;i++)put(m+i*t->result_bits/8,t->result_bits,0x12345678u+sample*17+i*53);
   for(unsigned i=0;i<input_n;i++){
    unsigned block_index=(i*t->input_bits/8)/16,x,y;
    if(t->input_bits==16){
     if(sample==0)x=y=0x8000;
     else if(sample==1){x=0xffff;y=1;}
     else if(sample==2){x=0x4000;y=1;}
     else if(sample==3){x=0xc000;y=1;}
     else{x=words[(sample+i*7+block_index*3)%12];y=words[(sample*3+i*5+block_index)%12];}
    }else{
     if(sample==0){x=255;y=128;}else if(sample==1){x=255;y=127;}else if(sample==2){x=128;y=255;}else if(sample==3){x=255;y=0;}
     else{x=bytes[(sample+i*7+block_index*3)%12];y=bytes[(sample*3+i*5+block_index)%12];}
    }
    put(a+i*t->input_bits/8,t->input_bits,x);put(b+i*t->input_bits/8,t->input_bits,y);
   }
   for(unsigned p=0;p<7;p++){
    uint64_t k=masks[p],saved=0;memset(out,0xa5,sizeof out);t->run(m,a,b,out,k);t->reference(m,a,b,reference,k);
    if(memcmp(out,reference,t->bytes)){printf("native %s sample%u mask%llx\n",t->name,sample,(unsigned long long)k);return 1;}
    for(unsigned i=0;i<n;i++){
     unsigned merge=get((t->alias==2?a:m)+i*t->result_bits/8,t->result_bits);
     unsigned expected=((k>>i)&1)?scalar(t->operation,a,t->alias==1?a:b,i):merge;
     if(get(out+i*t->result_bits/8,t->result_bits)!=expected){printf("scalar %s sample%u lane%u got%x expected%x\n",t->name,sample,i,get(out+i*t->result_bits/8,t->result_bits),expected);return 2;}
    }
    memcpy(&saved,out+256,8);if(memcmp(m,out+64,t->bytes)||memcmp(a,out+128,t->bytes)||memcmp(b,out+192,t->bytes)||saved!=k)return 3;
    if((t->bytes<64&&out[t->bytes]!=0xa5)||out[264]!=0xa5)return 4;
   }
  }
 }
 return 0;
}
