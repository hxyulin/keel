/* Arbitrary retained false lanes, both live inputs, result/memory aliases,
   exact guard boundaries, unaligned access and canonical observed masks. */
static int check_select(void(*fn)(const void*,void*),unsigned kind,unsigned observed){
 const uint64_t edges[]={0,1,2,0xffffffffULL,0x100000000ULL,0x7fffffff00000000ULL,0x7fffffffffffffffULL,0x8000000000000000ULL,0x8000000000000001ULL,0xffffffff00000000ULL,UINT64_MAX};
 for(unsigned n=0;n<1400;n++)for(unsigned off=0;off<16;off++)for(unsigned alias=0;alias<3;alias++){
  uint64_t x[2],f[2],got[8];unsigned char output[65];
  for(unsigned l=0;l<2;l++){
   x[l]=n<121?edges[(n/(l?11:1))%11]:next();
   f[l]=n<121?edges[(n+3+l*5)%11]:next();
   if(kind==1)f[l]=13;
   if(kind==2)f[l]=3;
   if(kind==3)f[l]=x[l];
  }
  unsigned char *src=region+page-32-off-(alias?80:0);
  memcpy(src,x,16);memcpy(src+16,f,16);
  unsigned char *out=alias?src+(alias==2?16:0):output+1;
  fn(src,out);memcpy(got,out,observed?64:48);
  for(unsigned l=0;l<2;l++){
   uint64_t mask=(x[l]>>63)?UINT64_MAX:0;
   uint64_t want=mask?x[l]:f[l];
   if(got[l]!=want||got[l+2]!=x[l]||got[l+4]!=f[l])return 1;
   if(observed&&got[l+6]!=mask)return 2;
  }
 }
 return 0;
}
