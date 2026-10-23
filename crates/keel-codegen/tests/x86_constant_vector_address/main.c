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
 /* Compare the whole arena after overlapping stores, including mode 1's
    reload after the first store has modified the input. */
 const int deltas[]={-16,-1,0,1,8};
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++) {
  const struct Case *c=&cases[ci];
  for(unsigned di=0;di<sizeof(deltas)/sizeof(deltas[0]);di++) {
   unsigned char arena[128],expected[128],first[32],again[16];
   memset(arena,0xa5,sizeof(arena));
   int32_t input[4]={INT32_MIN,INT32_MAX,-1,1};
   memcpy(arena+48,input,16); memcpy(expected,arena,sizeof(arena));
   unsigned out=48+deltas[di];
   if(c->wide) { for(unsigned i=0;i<4;i++){int64_t x=input[i];memcpy(first+i*8,&x,8);} }
   else memcpy(first,input,16);
   memcpy(expected+out,first,c->wide?32:16);
   if(c->mode==1) { memcpy(again,expected+48,16);memcpy(expected+out,again,16); }
   if(c->mode==3) memcpy(expected+out,input,16);
   unsigned before=csr();
   c->run((void*)((uintptr_t)(arena+48)-(uintptr_t)c->offset),arena+out);
   if(csr()!=before||memcmp(arena,expected,sizeof(arena))) {
    fprintf(stderr,"alias case=%u delta=%d\n",ci,deltas[di]);return 3;
   }
  }
 }
 /* A crossing input must fault before any observable output store. Shared
    output survives fork; handlers exit immediately to avoid core-dump work. */
 unsigned char *shared=mmap(0,page,PROT_READ|PROT_WRITE,MAP_SHARED|MAP_ANONYMOUS,-1,0);
 if(shared==MAP_FAILED)return 4;
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++) for(unsigned edge=0;edge<2;edge++) {
  memset(shared,0xa5,96);
  const struct Case *c=&cases[ci];
  unsigned char *p=edge?m+2*page-15:m+page-1;
  pid_t child=fork();if(child<0)return 5;
  if(!child) { signal(SIGSEGV,input_fault);signal(SIGBUS,input_fault);c->run((void*)((uintptr_t)p-(uintptr_t)c->offset),shared+17);_exit(0); }
  int status;if(waitpid(child,&status,0)!=child)return 6;
  if(!WIFEXITED(status)||(WEXITSTATUS(status)!=91&&WEXITSTATUS(status)!=92))return 7;
  for(unsigned i=0;i<96;i++)if(shared[i]!=0xa5)return 8;
 }
 munmap(shared,page);
 munmap(m,page*3);puts("PASS exact16 disp32: wrapping/extrema, unaligned, guards, live values, canaries, MXCSR");return 0;
}
