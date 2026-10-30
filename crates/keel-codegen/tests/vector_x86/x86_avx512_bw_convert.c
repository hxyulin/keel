#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
@DECLS@
@REFERENCES@
struct Case{const char*name;void(*run)(const void*,void*,void*,uint64_t);void(*reference)(const void*,void*,void*,uint64_t);unsigned produced,input_bytes,output_bytes,operation,kind,alias;};
static const struct Case cases[]={
@TABLE@
};
static unsigned word(const unsigned char*p){unsigned x=0;memcpy(&x,p,2);return x;}
static void put(unsigned char*p,unsigned x){memcpy(p,&x,2);}
static unsigned convert(unsigned operation,unsigned raw){
 if(operation==0)return (raw>=128?raw+0xff00:raw)&0xffff;
 if(operation==1)return raw;
 if(operation==2)return raw&255;
 if(operation==3){int signed_value=raw>=32768?(int)raw-65536:(int)raw;return (unsigned)(signed_value<-128?-128:signed_value>127?127:signed_value)&255;}
 return raw>255?255:raw;
}
static int check_output(const struct Case*t,const unsigned char*merge,const unsigned char*input,const unsigned char*out,uint64_t mask){
 unsigned lanes=t->operation<2?t->output_bytes/2:t->output_bytes;
 for(unsigned i=0;i<lanes;i++){
  unsigned expected=0,actual=t->operation<2?word(out+i*2):out[i];
  if(i<t->produced){unsigned old=t->operation<2?word(merge+i*2):merge[i];unsigned value=t->operation<2?input[i]:word(input+i*2);expected=(mask>>i)&1?convert(t->operation,value):old;}
  if(actual!=expected){printf("%s lane%u mask%llx got%x expected%x\n",t->name,i,(unsigned long long)mask,actual,expected);return 1;}
 }
 return 0;
}
int main(void){
 unsigned words[]={0,1,127,128,255,256,0x7fff,0x8000,0xffff,0xff7f,0xff80,0xff81,257,254,0x7ffe,0x8001};
 unsigned bytes[]={0,1,127,128,255,129,254,42,84,85,170,196,13,29,63,96};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;if(t->kind==1||t->kind==3)continue;
  uint64_t all=(UINT64_C(1)<<t->produced)-1,masks[]={0,all,UINT64_C(0xa55a8001),1,UINT64_C(1)<<(t->produced-1)};
  for(unsigned sample=0;sample<16;sample++){
   unsigned char m[64],a[64],out[224],reference[64];memset(m,0x6b,sizeof m);memset(a,0x4d,sizeof a);
   for(unsigned i=0;i<t->output_bytes;i++)m[i]=(unsigned char)(0xa7+sample*7+i*13);
   if(t->operation<2){for(unsigned i=0;i<t->input_bytes;i++)a[i]=(unsigned char)bytes[(sample+i)%16];}
   else{for(unsigned i=0;i<t->input_bytes/2;i++)put(a+i*2,words[(sample+i)%16]);}
   for(unsigned p=0;p<5;p++){
    uint64_t mask=masks[p],saved=0;memset(out,0xa5,sizeof out);t->run(m,a,out,mask);t->reference(m,a,reference,mask);
    if(memcmp(out,reference,t->output_bytes)){printf("native %s sample%u mask%llx\n",t->name,sample,(unsigned long long)mask);return 2;}
    if(check_output(t,m,t->alias?m:a,out,mask))return 3;
    memcpy(&saved,out+192,8);
    if(memcmp(out+64,m,t->output_bytes)||memcmp(out+128,t->alias?m:a,t->input_bytes)||saved!=mask)return 4;
    if((t->output_bytes<64&&out[t->output_bytes]!=0xa5)||out[200]!=0xa5)return 5;
   }
  }
 }
 long page=sysconf(_SC_PAGESIZE);
 unsigned char*map=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 unsigned char*refmap=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(map==MAP_FAILED||refmap==MAP_FAILED||mprotect(map+page,page,PROT_READ|PROT_WRITE)||mprotect(refmap+page,page,PROT_READ|PROT_WRITE))return 6;
 unsigned char*valid=map+page,*refvalid=refmap+page;
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;if(t->kind!=1&&t->kind!=3)continue;
  unsigned char merge[64],out[224],reference[64];
  for(unsigned i=0;i<64;i++)merge[i]=(unsigned char)(0xb7+i*13);
  if(t->kind==3){for(unsigned i=0;i<t->input_bytes/2;i++)put(merge+i*2,words[i%16]);}
  uint64_t all=(UINT64_C(1)<<t->produced)-1;
  for(unsigned p=0;p<5;p++){
   uint64_t mask=p==0?0:p==1?1:p==2?UINT64_C(1)<<(t->produced-1):p==3?all:all&UINT64_C(0xa55a8001);
   unsigned char*pointer=p==0?(unsigned char*)1:p==1?valid+page-1:p==2?valid-t->produced+1:valid+page-t->produced;
   unsigned char*refpointer=p==0?(unsigned char*)1:p==1?refvalid+page-1:p==2?refvalid-t->produced+1:refvalid+page-t->produced;
   memset(valid,0xa5,page);memset(refvalid,0xa5,page);memset(out,0xa5,sizeof out);
   if(t->kind==1&&p){for(unsigned i=0;i<t->produced;i++)if((mask>>i)&1){pointer[i]=(unsigned char)(0x80+i*13);refpointer[i]=pointer[i];}}
   t->run(merge,pointer,out,mask);t->reference(merge,refpointer,reference,mask);
   if(memcmp(valid,refvalid,page))return 7;
   uint64_t saved=0;memcpy(&saved,out+192,8);
   unsigned original=t->kind==3?t->input_bytes:t->output_bytes;
   if(memcmp(merge,out+64,original)||saved!=mask)return 8;
   if(t->kind==1){
    if(memcmp(out,reference,t->output_bytes))return 9;
    for(unsigned i=0;i<t->produced;i++){unsigned actual=word(out+i*2);unsigned expected=(mask>>i)&1?convert(t->operation,pointer[i]):word(merge+i*2);if(actual!=expected)return 10;}
   }else{
    for(unsigned i=0;i<t->produced;i++)if((mask>>i)&1){if(pointer[i]!=convert(t->operation,word(merge+i*2)))return 11;}
   }
  }
  if(t->kind==3){
   // Destination bytes overlap the source words. The source SSA value and its
   // spill home must preserve the complete pre-store word vector.
   for(unsigned p=0;p<2;p++){
    uint64_t mask=p?all:all&UINT64_C(0xa55a8001);unsigned char actual[64],expected[64],before[64];
    memcpy(actual,merge,64);memcpy(expected,merge,64);memcpy(before,merge,64);memset(out,0xa5,sizeof out);
    t->run(actual,actual+1,out,mask);t->reference(before,expected+1,reference,mask);
    if(memcmp(actual,expected,64)||memcmp(out+64,before,t->input_bytes))return 12;
   }
  }
 }
 return 0;
}
