#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
@DECLS@
@REFERENCES@
struct Case {const char*name;void(*run)(const void*,const void*,void*,uint64_t);void(*reference)(const void*,const void*,void*,uint64_t);unsigned bytes,bits,op,predicate,alias;};
static const struct Case cases[]={
@TABLE@
};
static unsigned lane(const unsigned char*p,unsigned bits){unsigned x=0;memcpy(&x,p,bits/8);return x;}
static void put(unsigned char*p,unsigned bits,unsigned x){memcpy(p,&x,bits/8);}
static int compare(unsigned predicate,int x,int y){
 switch(predicate){case 0:return x==y;case 1:return x<y;case 2:return x<=y;case 3:return 0;case 4:return x!=y;case 5:return x>=y;case 6:return x>y;case 7:return 1;}
 return 0;
}
int main(void){
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=cases+c;unsigned n=t->bytes*8/t->bits,step=t->bits/8;
  uint64_t all=n==64?UINT64_MAX:(UINT64_C(1)<<n)-1;
  uint64_t masks[]={0,all,UINT64_C(0xa55a8001c33c8001),1,UINT64_C(1)<<(n-1)};
  unsigned maximum=(1u<<t->bits)-1,sign=1u<<(t->bits-1);
  unsigned edges[]={0,1,maximum,sign,sign-1,maximum-1,sign+1,0x5555&maximum,0xaaaa&maximum};
  for(unsigned sample=0;sample<9;sample++){
   unsigned char a[64],b[64],out[336],reference[64];
   for(unsigned i=0;i<n;i++){put(a+i*step,t->bits,edges[(sample+i)%9]);put(b+i*step,t->bits,edges[(sample*3+i*5+1)%9]);}
   for(unsigned m=0;m<5;m++){
    uint64_t k=masks[m],expected=0,actual=0,saved=0;
    memset(out,0xa5,sizeof out);t->run(a,b,out,k);t->reference(a,b,reference,k);
    if(memcmp(out,reference,t->op==5?t->bytes:8))return 6;
    for(unsigned i=0;i<n;i++){
     unsigned x=lane(a+i*step,t->bits),y=lane((t->alias?a:b)+i*step,t->bits);
     int sx=x>=sign?(int)x-(1<<t->bits):(int)x,sy=y>=sign?(int)y-(1<<t->bits):(int)y;
     int selected=0;
     switch(t->op){
      case 0:selected=compare(t->predicate,sx,sy)&&((k>>i)&1);break;
      case 1:selected=compare(t->predicate,x,y)&&((k>>i)&1);break;
      case 2:selected=(x&y)!=0&&((k>>i)&1);break;
      case 3:selected=(x&y)==0&&((k>>i)&1);break;
      case 4:selected=(x&sign)!=0;break;
      case 5:selected=(k>>i)&1;break;
     }
     expected|=(uint64_t)selected<<i;
     if(t->op==5&&lane(out+i*step,t->bits)!=(selected?maximum:0))return 1;
     if(t->op!=5&&lane(out+256+i*step,t->bits)!=(selected?lane(b+i*step,t->bits):x))return 2;
    }
    memcpy(&actual,out+(t->op==5?256:0),8);memcpy(&saved,out+192,8);
    if(actual!=expected){printf("%s expected%llx actual%llx mask%llx sample%u\n",t->name,(unsigned long long)expected,(unsigned long long)actual,(unsigned long long)k,sample);return 3;}
    if(memcmp(a,out+64,t->bytes)||memcmp(b,out+128,t->bytes)||saved!=k)return 4;
    unsigned end=t->op==5?t->bytes:8;
    if((end<64&&out[end]!=0xa5)||out[200]!=0xa5||out[320]!=0xa5)return 5;
   }
  }
 }
 return 0;
}
