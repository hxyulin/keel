static void reference(const struct Case*c,const unsigned char*a,const unsigned char*b,unsigned char*out){
 unsigned carry=b[c->n]&1;for(unsigned j=0;j<c->n;j++){
 if(c->kind){int d=(int)a[j]-(int)b[j]-(int)carry;out[j]=(unsigned char)d;carry=d<0;}
 else{unsigned sum=(unsigned)a[j]+b[j]+carry;out[j]=(unsigned char)sum;carry=sum>>8;}}
 out[c->n]=(unsigned char)carry;
}
extern int fork(void);extern int waitpid(int,int*,int);extern void _exit(int);
static unsigned long calls,faults;
static int check(const struct Case*c,unsigned char*a,unsigned char*b,unsigned char*out,unsigned char*whole,unsigned off){
 unsigned char original_a[32]={0},original_b[32]={0},expected[32]={0},hardware[32] __attribute__((aligned(32)))={0},w[64],before[192];
 if(c->a)memcpy(original_a,a,c->a);if(c->b)memcpy(original_b,b,c->b);
 memset(w,0x79,64);memcpy(before,whole,192);reference(c,original_a,original_b,expected);
 unsigned state=csr();c->hardware(a,b,hardware);
 if(memcmp(expected,hardware,c->n+1)||csr()!=state){printf("FAIL hardware %s\n",c->name);return 1;}
 memcpy(before+off,expected,c->n);c->keel(a,b,out,w);calls++;
 if(memcmp(before,whole,192)||csr()!=state){printf("FAIL result/width %s\n",c->name);return 2;}
 unsigned char want[64];memset(want,0x79,64);if(c->a)memcpy(want,original_a,c->a);if(c->b)memcpy(want+32,original_b,c->b);
 want[16]=expected[c->n];
 if(memcmp(w,want,64)){printf("FAIL live %s\n",c->name);return 3;}return 0;
}
static int fault(const struct Case*c,unsigned char*a,unsigned char*b,unsigned char*out){
 int pid=fork(),status=0;if(pid<0)return 30;if(!pid){unsigned char w[64];c->keel(a,b,out,w);_exit(0);}
 if(waitpid(pid,&status,0)!=pid)return 31;unsigned signal=status&127;if(signal!=11&&signal!=7){printf("FAIL missing fault %s status=%d\n",c->name,status);return 32;}faults++;return 0;
}
int main(void){unsigned long page=(unsigned long)getpagesize();unsigned char*maps[3];
 for(unsigned j=0;j<3;j++){maps[j]=mmap(0,page*3,0,2|32,-1,0);if(maps[j]==(void*)-1||mprotect(maps[j]+page,page,3))return 20;}
 unsigned long long random=0x91543a2f;
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){
  const struct Case*c=&cases[ci];
  for(unsigned pattern=0;pattern<544;pattern++)for(unsigned alias=0;alias<4;alias++)for(unsigned cin=0;cin<2;cin++){
   unsigned char aa[192] __attribute__((aligned(32))),bb[192] __attribute__((aligned(32))),oo[192] __attribute__((aligned(32)));
   memset(aa,0xa5,192);memset(bb,0xa5,192);memset(oo,0xa5,192);
   unsigned off=64+(c->align==1?pattern%32:0);unsigned char*a=aa+off,*b=bb+off,*out=(alias==1?aa:alias==2?bb:oo)+off;
   for(unsigned j=0;j<32;j++){
    random=random*6364136223846793005ULL+1;
    unsigned pos=pattern%256;unsigned char bit=(unsigned char)(j==pos/8?1u<<(pos%8):0);
    a[j]=pattern<256?bit:pattern<512?(unsigned char)~bit:(unsigned char)(random>>32);
    b[j]=pattern<256?(unsigned char)~bit:pattern<512?bit:(unsigned char)(random>>40);
    if(pattern>=512&&pattern<520){static const unsigned char edge[]={0,255,128,127,1,254,85,170};a[j]=edge[pattern-512];b[j]=edge[(pattern-512+3)%8];}
   }
   a[c->n]=(unsigned char)cin;b[c->n]=(unsigned char)cin;
   if(alias==3)b=a;
   int r=check(c,a,b,out,alias==1?aa:alias==2?bb:oo,off);if(r)return r;
  }
  for(unsigned side=0;side<2;side++){
   for(unsigned j=0;j<3;j++)if(mprotect(maps[j]+page,page,3))return 21;
   unsigned char*a=maps[0]+page+(side?page-c->a:0),*b=maps[1]+page+(side?page-c->b:0),*out=maps[2]+page+(side?page-c->n:0),expected[32],hardware[32] __attribute__((aligned(32))),w[64];
   for(unsigned j=0;j<c->a;j++)a[j]=(unsigned char)(j*37+11);for(unsigned j=0;j<c->b;j++)b[j]=(unsigned char)(j*41+173);
   b[c->n]=(unsigned char)side;
   unsigned char ac[32]={0},bc[32]={0};if(c->a)memcpy(ac,a,c->a);if(c->b)memcpy(bc,b,c->b);reference(c,ac,bc,expected);
   c->hardware(a,b,hardware);if(memcmp(expected,hardware,c->n+1))return 22;
   if(mprotect(maps[0]+page,page,c->a?1:0)||mprotect(maps[1]+page,page,c->b?1:0))return 23;
   memset(w,0x79,64);unsigned state=csr();c->keel(a,b,out,w);calls++;
   unsigned char want[64];memset(want,0x79,64);if(c->a)memcpy(want,ac,c->a);if(c->b)memcpy(want+32,bc,c->b);
   want[16]=expected[c->n];
   if(memcmp(expected,out,c->n)||memcmp(w,want,64)||csr()!=state)return 24;
   // Only the IR recipe is used for crossing probes. Aligned hardware forms
   // have a precondition, and no hardware misalignment-fault claim is made.
   long delta=side?1:-1;
   if(c->a&&fault(c,a+delta,b,out))return 25;
   if(c->b&&fault(c,a,b+delta,out))return 26;
   if(fault(c,a,b,out+delta))return 27;
  }
 }
 for(unsigned j=0;j<3;j++)if(munmap(maps[j],page*3))return 28;
 unsigned long count=sizeof(cases)/sizeof(*cases);if(calls!=count*4354||faults!=count*6)return 40;
 printf("PASS carry-borrow cases=%lu calls=%lu faults=%lu hardware-and-byte-carry\n",count,calls,faults);return 0;
}
