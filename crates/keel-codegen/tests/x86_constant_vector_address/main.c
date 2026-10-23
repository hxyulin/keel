int main(void) {
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *m=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(m==MAP_FAILED||mprotect(m+page,page,PROT_READ|PROT_WRITE)) return 2;
 const int32_t values[]={INT32_MIN,INT32_MAX,-1,0,1,-1234567,987654321};
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++) {
  const struct Case *c=&cases[ci];
  for(unsigned edge=0;edge<34;edge++) for(unsigned seed=0;seed<7;seed++) {
   unsigned char *p=edge==0?m+page:edge==1?m+2*page-16:m+page+edge-1;
   int32_t input[4]; for(unsigned i=0;i<4;i++)input[i]=values[(i+seed)%7];
   memcpy(p,input,16);
   unsigned char output[96],expected[96]; memset(output,0xa5,sizeof(output));memset(expected,0xa5,sizeof(expected));
   if(c->wide) { for(unsigned i=0;i<4;i++){int64_t x=input[i];memcpy(expected+17+i*8,&x,8);} }
   else memcpy(expected+17,input,16);
   if(c->mode==1||c->mode==3)memcpy(expected+17,input,16);
   unsigned before=csr();
   c->run((void*)((uintptr_t)p-(uintptr_t)c->offset),output+17);
   if(csr()!=before||memcmp(output,expected,sizeof(output))||memcmp(p,input,16)) {
    fprintf(stderr,"case=%u edge=%u seed=%u\n",ci,edge,seed);return 1;
   }
  }
 }
 munmap(m,page*3);puts("PASS exact16 disp32: wrapping/extrema, unaligned, guards, live values, canaries, MXCSR");return 0;
}
