/* Independent unsigned lane oracle: never shifts by an out-of-range count. */
static void reference(int op, const unsigned char *a, uint64_t count, unsigned char *out) {
  int bits = (int[]){16,32,64,16,32,64,16,32}[op];
  uint64_t mask = UINT64_MAX >> (64-bits);
  for(int lane=0;lane<128/bits;lane++) {
    uint64_t x=0,r; memcpy(&x,a+lane*bits/8,bits/8);
    if(op<3) r=count>=(unsigned)bits?0:(x<<count)&mask;
    else if(op<6) r=count>=(unsigned)bits?0:x>>count;
    else {
      int negative=(x>>(bits-1))&1;
      if(count>=(unsigned)bits) r=negative?mask:0;
      else {r=x>>count;if(negative&&count)r|=mask^(mask>>count);}
    }
    memcpy(out+lane*bits/8,&r,bits/8);
  }
}
static int run(void) {
  uint64_t counts[261];
  for(unsigned i=0;i<256;i++)counts[i]=i;
  counts[256]=256;counts[257]=65536;counts[258]=UINT64_C(1)<<32;counts[259]=UINT64_C(1)<<63;counts[260]=UINT64_MAX;
  long page=sysconf(_SC_PAGESIZE);
  unsigned char *maps[3],*ptr[3];
  for(int k=0;k<3;k++) {
    maps[k]=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if(maps[k]==MAP_FAILED||mprotect(maps[k]+page,page,PROT_READ|PROT_WRITE))return 1;
  }
  for(int edge=0;edge<3;edge++) {
    for(int k=0;k<3;k++)ptr[k]=maps[k]+page+(edge==0?0:edge==1?page-16:3);
    for(int pattern=0;pattern<8;pattern++) {
      unsigned char a[16],expect[16];
      for(int k=0;k<16;k++)a[k]=pattern==0?0:pattern==1?255:pattern==2?(k%2?128:0):pattern==3?(k*37+11):(k==0?1:0);
      for(int op=0;op<8;op++)for(unsigned j=0;j<sizeof(counts)/sizeof(*counts);j++) {
        if(pattern>=5) {
          int bits=(int[]){16,32,64,16,32,64,16,32}[op];
          uint64_t sign=UINT64_C(1)<<(bits-1);
          uint64_t value=pattern==5?sign-1:pattern==6?sign:1;
          for(int lane=0;lane<128/bits;lane++)memcpy(a+lane*bits/8,&value,bits/8);
        }
        uint64_t c[2]={counts[j],UINT64_MAX}; V av,nv,z,v;
        memcpy(&av,a,16);memcpy(&nv,c,16);reference(op,a,c[0],expect);
        z=shifts[op](av,nv);v=vendors[op](av,nv);
        if(memcmp(&z,expect,16)||memcmp(&v,expect,16)){fprintf(stderr,"op %d count %llu\n",op,(unsigned long long)c[0]);return 2;}
        memset(maps[2]+page,0xa5,page);memcpy(ptr[0],a,16);memcpy(ptr[1],c,16);
        evaluations=0;memories[op](ptr[2],ptr[0],ptr[1]);
        if(evaluations!=2||memcmp(ptr[2],expect,16))return 3;
        for(long k=0;k<page;k++)if((maps[2]+page+k<ptr[2]||maps[2]+page+k>=ptr[2]+16)&&maps[2][page+k]!=0xa5)return 4;
        /* Aliased output and input; unaligned and protected-page edges. */
        evaluations=0;memories[op](ptr[0],ptr[0],ptr[1]);
        if(evaluations!=2||memcmp(ptr[0],expect,16))return 5;
        /* The output may also overwrite the count after both inputs are read. */
        memcpy(ptr[0],a,16);memcpy(ptr[1],c,16);
        evaluations=0;memories[op](ptr[1],ptr[0],ptr[1]);
        if(evaluations!=2||memcmp(ptr[1],expect,16))return 8;
        V x=callback(av,nv),want=_mm_sll_epi16(x,nv);z=direct(av,nv);if(memcmp(&z,&want,16))return 6;
        want=_mm_srl_epi64(x,nv);z=indirect(av,nv,callback);if(memcmp(&z,&want,16))return 7;
      }
    }
  }
  for(int k=0;k<3;k++)munmap(maps[k],page*3);
  return 0;
}
