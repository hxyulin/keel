static int check(const struct Case *c,const u8 *a,const u8 *b,u8 *out) {
 u8 x[16],y[16],expected[48];
 memcpy(x,a,16);memcpy(y,b,16);
 memcpy(expected,x+8,8);memcpy(expected+8,c->same?x:y,8);
 memcpy(expected+16,x,16);memcpy(expected+32,y,16);
 c->run(a,b,out);
 if(memcmp(out,expected,c->live?48:16)){printf("failed %s\n",c->name);return 1;}
 return 0;
}
int main(void) {
 const u64 edges[]={0,1,~0ULL,0x8000000000000000ULL,0x7ff0000000000000ULL,
  0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0123456789abcULL,0xfff0123456789abcULL};
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++)
 for(unsigned pattern=0;pattern<265;pattern++)
 for(unsigned offset=0;offset<16;offset++) for(unsigned alias=0;alias<4;alias++) {
  u8 a[128],b[128],dest[128];memset(a,0x5a,sizeof(a));memset(b,0x5a,sizeof(b));memset(dest,0x5a,sizeof(dest));
  for(unsigned i=0;i<16;i++) {a[32+offset+i]=(u8)(pattern+31*i);b[32+offset+i]=(u8)(pattern+71*i+93);}
  if(pattern>=256) for(unsigned l=0;l<2;l++) {
   u64 x=edges[(pattern-256+l)%9],y=edges[(pattern-256+4+l)%9];
   memcpy(a+32+offset+8*l,&x,8);memcpy(b+32+offset+8*l,&y,8);
  }
  const u8 *pa=a+32+offset,*pb=alias==3?pa:b+32+offset;
  u8 *out=(alias==1?a:alias==2?b:dest)+32+offset;
  unsigned size=cases[c].live?48:16;
  // Set output sentinels without touching either captured input.
  u8 before=out[-1],after=out[size];
  if(check(&cases[c],pa,pb,out))return 1;
  if(out[-1]!=before||out[size]!=after)return 2;
 }
#ifdef __APPLE__
 const int anon=0x1000, sc_pagesize=29;
#else
 const int anon=0x20, sc_pagesize=30;
#endif
 usize n=(usize)sysconf(sc_pagesize);
 u8 *a=mmap(0,3*n,3,2|anon,-1,0),*b=mmap(0,3*n,3,2|anon,-1,0),*o=mmap(0,3*n,3,2|anon,-1,0);
 if(a==(void*)-1||b==(void*)-1||o==(void*)-1)return 3;
 if(mprotect(a,n,0)||mprotect(a+2*n,n,0)||mprotect(b,n,0)||mprotect(b+2*n,n,0)||mprotect(o,n,0)||mprotect(o+2*n,n,0))return 4;
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++)
 for(unsigned side=0;side<4;side++) for(unsigned offset=0;offset<16;offset++) {
  // Each source independently meets the lower/upper guard; full 16-byte IR loads.
  u8 *pa=side&1?a+2*n-16-offset:a+n+offset;
  u8 *pb=side&2?b+2*n-16-offset:b+n+offset;
  unsigned size=cases[c].live?48:16;
  u8 *out=o+2*n-size-offset;
  for(unsigned i=0;i<16;i++){pa[i]=(u8)(i*31+128);pb[i]=(u8)(i*71+93);}
  if(check(&cases[c],pa,pb,out))return 5;
 }
 munmap(a,3*n);munmap(b,3*n);munmap(o,3*n);
 printf("passed %u window cases: raw bits, NaNs, aliases, live sources, spills, unaligned and guarded reads\n",(unsigned)(sizeof(cases)/sizeof(cases[0])));
 return 0;
}
