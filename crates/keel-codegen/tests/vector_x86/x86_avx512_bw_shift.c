#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
@DECLS@
@REFERENCES@
struct Case{const char*name;void(*run)(const void*,const void*,const void*,void*,uint64_t);void(*reference)(const void*,const void*,const void*,void*,uint64_t);unsigned bytes,direction,kind,immediate,alias;};
static const struct Case cases[]={
@TABLE@
};
static unsigned word(const unsigned char*p){unsigned x=0;memcpy(&x,p,2);return x;}
static void put(unsigned char*p,unsigned x){memcpy(p,&x,2);}
static unsigned scalar(unsigned direction,unsigned x,uint64_t count){
 if(count>=16)return direction==2&&(x&0x8000)?0xffff:0;
 if(count==0)return x;
 if(direction==0)return (x<<count)&0xffff;
 if(direction==1||!(x&0x8000))return x>>count;
 return ((x>>count)|(0xffffu<<(16-count)))&0xffff;
}
int main(void){
 uint64_t counts[]={0,1,8,15,16,17,255,65536,UINT64_C(1)<<32,UINT64_C(1)<<63,UINT64_MAX};
 unsigned edge[]={0,1,2,15,16,17,0x8000,0xffff,0x7fff,0x8001,0x5555};
 unsigned variable[]={0,1,8,15,16,17,255,256,0x8000,0xffff,7};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;unsigned n=t->bytes/2;
  uint64_t all=(UINT64_C(1)<<n)-1,masks[]={0,all,UINT64_C(0xa55a8001),1,UINT64_C(1)<<(n-1)};
  for(unsigned sample=0;sample<11;sample++)for(unsigned upper=0;upper<(t->kind==1?2u:1u);upper++){
   unsigned char m[64],a[64],count[64],out[336],reference[64];
   for(unsigned i=0;i<n;i++){put(m+i*2,0x1234+sample*17+i*53);put(a+i*2,edge[(sample+i)%11]);put(count+i*2,variable[(sample*3+i*7)%11]);}
   if(t->kind==1){uint64_t high=upper?UINT64_MAX:0;memcpy(count,&counts[sample],8);memcpy(count+8,&high,8);}
   for(unsigned p=0;p<5;p++){
    uint64_t k=masks[p],saved=0;memset(out,0xa5,sizeof out);t->run(m,a,count,out,k);t->reference(m,a,count,reference,k);
    if(memcmp(out,reference,t->bytes)){printf("vendor %s sample%u upper%u mask%llx\n",t->name,sample,upper,(unsigned long long)k);return 1;}
    for(unsigned i=0;i<n;i++){
     unsigned input=word(a+i*2),merge=word((t->alias?a:m)+i*2);
     uint64_t amount=t->kind==0?t->immediate:t->kind==1?counts[sample]:t->alias?input:word(count+i*2);
     unsigned expected=(k>>i)&1?scalar(t->direction,input,amount):merge;
     if(word(out+i*2)!=expected){printf("scalar %s lane%u count%llu got%x expected%x\n",t->name,i,(unsigned long long)amount,word(out+i*2),expected);return 2;}
    }
    memcpy(&saved,out+256,8);
    unsigned count_bytes=t->kind==1?16:t->bytes;
    if(memcmp(out+64,m,t->bytes)||memcmp(out+128,a,t->bytes)||memcmp(out+192,count,count_bytes)||saved!=k)return 3;
    if((t->bytes<64&&out[t->bytes]!=0xa5)||out[264]!=0xa5)return 4;
   }
  }
 }
 return 0;
}
