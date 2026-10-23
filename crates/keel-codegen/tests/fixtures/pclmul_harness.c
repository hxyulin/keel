/* Independent XOR/shift polynomial reference, without undefined shifts. */
static void product(uint64_t a,uint64_t b,uint64_t out[2]) {
  out[0]=out[1]=0;
  for(unsigned i=0;i<64;i++)if((b>>i)&1){
    out[0]^=a<<i;
    if(i)out[1]^=a>>(64-i);
  }
}
#ifdef PCLMUL_C_EVAL
extern int eval_count;
#endif
static unsigned mxcsr(void){unsigned n;__asm__ volatile("stmxcsr %0":"=m"(n));return n;}
static void reference(const struct Case*c,const void*a,const void*b,unsigned char out[48]){
 uint64_t x[2],y[2],z[2];memcpy(x,a,16);memcpy(y,c->same?a:b,16);
 product(x[c->c&1],y[(c->c>>4)&1],z);
 memcpy(out,z,16);memcpy(out+16,a,16);memcpy(out+32,b,16);
}
int main(void){
#if defined(__GNUC__)
 __builtin_cpu_init();if(!__builtin_cpu_supports("pclmul")){puts("SKIP no pclmul");return 77;}
#endif
 unsigned csr=mxcsr();uint64_t rng=0xb78309da73c125ULL;
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){
  const struct Case*c=&cases[ci];
  /* Every pair of basis bits proves each coefficient path, plus edges/random. */
  for(unsigned n=0;n<4352;n++)for(unsigned alias=0;alias<3;alias++){
   unsigned char aa[96],bb[96],oo[96],expected[48];memset(aa,0xa5,96);memset(bb,0xa5,96);memset(oo,0xa5,96);
   unsigned off=1+(n&15);void*a=aa+off,*b=bb+off,*o=(alias==0?oo:alias==1?aa:bb)+off;
   uint64_t x[2],y[2];
   for(unsigned j=0;j<2;j++){
    if(n<4096){x[j]=1ULL<<(n&63);y[j]=1ULL<<((n>>6)&63);if(j!=(c->c&1))x[j]^=~0ULL;if(j!=((c->c>>4)&1))y[j]^=~0ULL;}
    else if(n<4100){const uint64_t edge[]={0,1,~0ULL,1ULL<<63};x[j]=edge[n-4096];y[j]=edge[4099-n];}
    else{rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;x[j]=rng;rng^=rng<<13;rng^=rng>>7;rng^=rng<<17;y[j]=rng;}
   }
   memcpy(a,x,16);memcpy(b,y,16);reference(c,a,b,expected);
#ifdef PCLMUL_C_EVAL
   eval_count=0;
#endif
   c->fn(a,b,o);
#ifdef PCLMUL_C_EVAL
   if(eval_count!=2)return 30;
#endif
   if(memcmp(o,expected,48)||((unsigned char*)o)[-1]!=0xa5||((unsigned char*)o)[48]!=0xa5||mxcsr()!=csr){printf("FAIL ci=%u n=%u alias=%u\n",ci,n,alias);return 10;}
  }
  long page=sysconf(_SC_PAGESIZE);unsigned char *maps[3];
  for(unsigned j=0;j<3;j++){maps[j]=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(maps[j]==MAP_FAILED||mprotect(maps[j]+page,page,PROT_READ|PROT_WRITE))return 20;}
  for(unsigned edge=0;edge<2;edge++){
   void*a=maps[0]+page+(edge?page-16:0),*b=maps[1]+page+(edge?page-16:0),*o=maps[2]+page+(edge?page-48:0);unsigned char e[48];
   memset(a,0x81,16);memset(b,0x5a,16);reference(c,a,b,e);c->fn(a,b,o);if(memcmp(o,e,48)||mxcsr()!=csr)return 21;
  }
  for(unsigned j=0;j<3;j++)munmap(maps[j],3*page);
 }
 puts("PASS pclmul: independent GF2 edges basis random controls aliases live calls guards MXCSR");return 0;
}
