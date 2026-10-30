extern int printf(const char *,...);
extern void cpu_initialize(void);
extern int dispatch_add(int *,const int *,const int *);
#define QUERY(name) extern int query_##name(void);
QUERY(monitor) QUERY(sse) QUERY(sse2) QUERY(sse3) QUERY(ssse3) QUERY(sse41) QUERY(sse42)
QUERY(avx) QUERY(avx2) QUERY(avx512f) QUERY(avx512vl) QUERY(avx512dq) QUERY(avx512bw) QUERY(avx512cd) QUERY(aes) QUERY(pclmul) QUERY(popcnt) QUERY(bmi)
QUERY(bmi2) QUERY(cx16) QUERY(movbe) QUERY(lzcnt) QUERY(xsave) QUERY(fma) QUERY(f16c)
static void cpuid(unsigned leaf,unsigned out[4]) {
  __asm__ volatile("cpuid":"=a"(out[0]),"=b"(out[1]),"=c"(out[2]),"=d"(out[3]):"a"(leaf),"c"(0));
}
struct Query { const char *name; int (*query)(void); unsigned leaf,reg,mask; unsigned long long state; };
static const struct Query queries[]={
#define ROW(name,leaf,reg,bit,state) {#name,query_##name,leaf,reg,1U<<bit,state}
 ROW(monitor,1,2,3,0),ROW(sse,1,3,25,0),ROW(sse2,1,3,26,0),ROW(sse3,1,2,0,0),
 ROW(ssse3,1,2,9,0),ROW(sse41,1,2,19,0),ROW(sse42,1,2,20,0),
 ROW(avx,1,2,28,6),ROW(avx2,7,1,5,6),
 {"avx512f",query_avx512f,7,1,1U<<16,0xe6},
 {"avx512vl",query_avx512vl,7,1,(1U<<16)|(1U<<31),0xe6},
 {"avx512dq",query_avx512dq,7,1,(1U<<16)|(1U<<17),0xe6},
 {"avx512bw",query_avx512bw,7,1,(1U<<16)|(1U<<30),0xe6},
 {"avx512cd",query_avx512cd,7,1,(1U<<16)|(1U<<28),0xe6},ROW(aes,1,2,25,0),
 ROW(pclmul,1,2,1,0),ROW(popcnt,1,2,23,0),ROW(bmi,7,1,3,0),
 ROW(bmi2,7,1,8,0),ROW(cx16,1,2,13,0),ROW(movbe,1,2,22,0),
 ROW(lzcnt,0x80000001U,2,5,0),{"xsave",query_xsave,1,2,(1U<<26)|(1U<<27),0},ROW(fma,1,2,12,6),ROW(f16c,1,2,29,6)
};
int main(void){
 unsigned basic[4],extended[4],leaf1[4]={0},row[4];
 cpuid(0,basic);cpuid(0x80000000U,extended);if(basic[0]>=1)cpuid(1,leaf1);
 unsigned long long xcr0=0;
 if((leaf1[2]&(7U<<26))==(7U<<26)){
  unsigned low,high;__asm__ volatile("xgetbv":"=a"(low),"=d"(high):"c"(0));xcr0=low|((unsigned long long)high<<32);
 }
 int avx_cpu=(leaf1[2]&(7U<<26))==(7U<<26);
 int avx=avx_cpu&&(xcr0&6)==6;
 for(unsigned i=0;i<sizeof(queries)/sizeof(queries[0]);++i){
  const struct Query *q=&queries[i];unsigned maximum=q->leaf&0x80000000U?extended[0]:basic[0];
  int expected=0;if(maximum>=q->leaf){cpuid(q->leaf,row);expected=(row[q->reg]&q->mask)==q->mask;}
  if(q->state)expected=expected&&avx_cpu&&(xcr0&q->state)==q->state;
  int actual=q->query();cpu_initialize();int repeated=q->query();
  if((actual!=0)!=expected||(repeated!=0)!=expected||actual<0||actual>1){printf("CPU query mismatch %s expected%d actual%d repeated%d\n",q->name,expected,actual,repeated);return 1;}
 }
 int a[8]={1,-1,42,-42,0,9000,7,-3},b[8]={9,21,-42,72,0,-9000,8,-1},out[8];
 int used=dispatch_add(out,a,b);
 if(used!=(avx&&((basic[0]>=7?(cpuid(7,row),row[1]):0)&(1U<<5))!=0))return 2;
 for(unsigned i=0;i<8;++i)if(out[i]!=a[i]+b[i])return 3;
 return 0;
}
