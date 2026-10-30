#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>
@DECLS@
struct Case { const char *name; void (*run)(const void*,const void*,const void*,void*,uint64_t); unsigned bytes,bits,op,kind,alias; };
static const struct Case cases[]={
@TABLE@
};
static unsigned lane(const unsigned char *p,unsigned bits){ unsigned x=0; memcpy(&x,p,bits/8); return x; }
static void put(unsigned char *p,unsigned bits,unsigned x){ memcpy(p,&x,bits/8); }
static unsigned scalar(unsigned op,unsigned x,unsigned y,unsigned bits) {
 unsigned maximum=(1u<<bits)-1; int high=(1<<(bits-1))-1,low=-(1<<(bits-1));
 int sx=x>=(1u<<(bits-1))?(int)x-(1<<bits):(int)x;
 int sy=y>=(1u<<(bits-1))?(int)y-(1<<bits):(int)y;
 int value=0;
 switch(op) {
 case 0:return (x+y)&maximum; case 1:return (x-y)&maximum;
 case 2:case 3:value=op==2?sx+sy:sx-sy;return (unsigned)(value<low?low:value>high?high:value)&maximum;
 case 4:return x+y>maximum?maximum:x+y; case 5:return x<y?0:x-y;
 case 6:return sx<sy?x:y; case 7:return x<y?x:y;
 case 8:return sx>sy?x:y; case 9:return x>y?x:y;
 default:return x;
 }
}
int main(void) {
 unsigned edges[]={0,1,0xffff,0x8000,0x7fff,0xff,0x80,0x7f,2,0xfffe};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++) {
  const struct Case *t=cases+c; if(t->kind>=2)continue;
  unsigned n=t->bytes*8/t->bits,step=t->bits/8;
  uint64_t all=n==64?UINT64_MAX:(UINT64_C(1)<<n)-1;
  uint64_t masks[]={0,all,UINT64_C(0xa55a8001c33c8001),1,UINT64_C(1)<<(n-1)};
  for(unsigned sample=0;sample<10;sample++) {
   unsigned char m[64],a[64],b[64],out[272];
   for(unsigned i=0;i<n;i++) {
    put(m+i*step,t->bits,0x1234+sample*17+i*7);
    put(a+i*step,t->bits,edges[(sample+i)%10]);
    put(b+i*step,t->bits,edges[(sample*3+i*7+1)%10]);
   }
   for(unsigned p=0;p<5;p++) {
    uint64_t k=masks[p],saved=0;memset(out,0xa5,sizeof out); t->run(m,a,b,out,k);
    for(unsigned i=0;i<n;i++) {
     unsigned merge=lane((t->alias?a:m)+i*step,t->bits);
     unsigned x=lane((t->alias?m:a)+i*step,t->bits),y=lane((t->alias?a:b)+i*step,t->bits);
     unsigned expected=(k>>i)&1?(t->kind==1?x:scalar(t->op,x,y,t->bits)):merge;
     if(lane(out+i*step,t->bits)!=expected) {printf("%s lane%u mask%llx sample%u got%x expected%x\n",t->name,i,(unsigned long long)k,sample,lane(out+i*step,t->bits),expected);return 1;}
    }
    memcpy(&saved,out+256,8);
    if(memcmp(out+64,m,t->bytes)||memcmp(out+128,a,t->bytes)||memcmp(out+192,b,t->bytes)||saved!=k)return 2;
    if((t->bytes<64&&out[t->bytes]!=0xa5)||out[264]!=0xa5)return 3;
   }
  }
 }
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *guard=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(guard==MAP_FAILED||mprotect(guard+page,page,PROT_READ|PROT_WRITE))return 4;
 unsigned char *valid=guard+page;
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++) {
  const struct Case*t=cases+c;if(t->kind<2)continue;
  unsigned n=t->bytes*8/t->bits,step=t->bits/8;
  unsigned char m[64],out[272];for(unsigned i=0;i<64;i++)m[i]=(unsigned char)(i*13+1);
  // Mask zero suppresses even an invalid pointer. Low/high single-lane masks
  // prove that preceding or following protected lanes are not accessed.
  for(unsigned p=0;p<5;p++) {
   if(p==4&&step!=2)continue;
   uint64_t k=p==0?0:(p==1||p==4)?1:p==2?UINT64_C(1)<<(n-1):n==64?UINT64_MAX:(UINT64_C(1)<<n)-1;
   unsigned active=p==2?n-1:0;
   unsigned char *pointer=p==0?(unsigned char*)1:p==1?valid+page-step:p==2?valid-(n-1)*step:p==4?valid+page-3:valid+page-t->bytes;
   memset(valid,0xa5,page);memset(out,0xa5,sizeof out);
   if(p&&t->kind==2) {if(p==3){for(unsigned i=0;i<n;i++)put(pointer+i*step,t->bits,0x8123);}else put(pointer+active*step,t->bits,0x8123);}
   t->run(m,pointer,m,out,k);
   if(t->kind==2) {
    for(unsigned i=0;i<n;i++) {
     unsigned expected=((k>>i)&1)?(0x8123&((1u<<t->bits)-1)):lane(m+i*step,t->bits);
     if(lane(out+i*step,t->bits)!=expected)return 5;
    }
   } else if(p) {
    if(p==3) {if(memcmp(pointer,m,t->bytes))return 6;if(pointer[-1]!=0xa5)return 7;}
    else if(lane(pointer+active*step,t->bits)!=lane(m+active*step,t->bits))return 6;
    if(p==1&&valid[page-step-1]!=0xa5)return 7;
    if(p==2&&valid[step]!=0xa5)return 8;
    if(p==4&&(valid[page-1]!=0xa5||valid[page-4]!=0xa5))return 8;
   }
   if(memcmp(out+64,m,t->bytes))return 9;
  }
 }
 return 0;
}
