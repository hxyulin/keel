extern int printf(const char *,...);
extern void cpu_initialize(void);
extern int dispatch_add(int *,const int *,const int *);
#define QUERY(name) extern int query_##name(void);
QUERY(monitor) QUERY(sse) QUERY(sse2) QUERY(sse3) QUERY(ssse3) QUERY(sse41) QUERY(sse42)
QUERY(avx) QUERY(avx2) QUERY(aes) QUERY(pclmul) QUERY(popcnt) QUERY(bmi)
QUERY(bmi2) QUERY(cx16) QUERY(movbe) QUERY(lzcnt) QUERY(xsave) QUERY(fma) QUERY(f16c)
static void cpuid(unsigned leaf,unsigned out[4]) {
  __asm__ volatile("cpuid":"=a"(out[0]),"=b"(out[1]),"=c"(out[2]),"=d"(out[3]):"a"(leaf),"c"(0));
}
struct Query { const char *name; int (*query)(void); unsigned leaf,reg,bit,avx; };
static const struct Query queries[]={
#define ROW(name,leaf,reg,bit,avx) {#name,query_##name,leaf,reg,bit,avx}
 ROW(monitor,1,2,3,0),ROW(sse,1,3,25,0),ROW(sse2,1,3,26,0),ROW(sse3,1,2,0,0),
 ROW(ssse3,1,2,9,0),ROW(sse41,1,2,19,0),ROW(sse42,1,2,20,0),
 ROW(avx,1,2,28,1),ROW(avx2,7,1,5,1),ROW(aes,1,2,25,0),
 ROW(pclmul,1,2,1,0),ROW(popcnt,1,2,23,0),ROW(bmi,7,1,3,0),
 ROW(bmi2,7,1,8,0),ROW(cx16,1,2,13,0),ROW(movbe,1,2,22,0),
 ROW(lzcnt,0x80000001U,2,5,0),ROW(xsave,1,2,26,2),ROW(fma,1,2,12,1),ROW(f16c,1,2,29,1)
};
int main(void){
 unsigned basic[4],extended[4],leaf1[4]={0},row[4];
 cpuid(0,basic);cpuid(0x80000000U,extended);if(basic[0]>=1)cpuid(1,leaf1);
 unsigned long long xcr0=0;
 if((leaf1[2]&(3U<<26))==(3U<<26)){
  unsigned low,high;__asm__ volatile("xgetbv":"=a"(low),"=d"(high):"c"(0));xcr0=low|((unsigned long long)high<<32);
 }
 int avx=(leaf1[2]&(1U<<28))!=0&&(xcr0&6)==6;
 for(unsigned i=0;i<sizeof(queries)/sizeof(queries[0]);++i){
  const struct Query *q=&queries[i];unsigned maximum=q->leaf&0x80000000U?extended[0]:basic[0];
  int expected=0;if(maximum>=q->leaf){cpuid(q->leaf,row);expected=(row[q->reg]>>q->bit)&1;}
  if(q->avx==1)expected=expected&&avx;
  if(q->avx==2)expected=expected&&((leaf1[2]&(3U<<26))==(3U<<26));
  int actual=q->query();cpu_initialize();int repeated=q->query();
  if((actual!=0)!=expected||(repeated!=0)!=expected||actual<0||actual>1){printf("CPU query mismatch %s expected%d actual%d repeated%d\n",q->name,expected,actual,repeated);return 1;}
 }
 int a[8]={1,-1,42,-42,0,9000,7,-3},b[8]={9,21,-42,72,0,-9000,8,-1},out[8];
 int used=dispatch_add(out,a,b);
 if(used!=(avx&&((basic[0]>=7?(cpuid(7,row),row[1]):0)&(1U<<5))!=0))return 2;
 for(unsigned i=0;i<8;++i)if(out[i]!=a[i]+b[i])return 3;
 return 0;
}
