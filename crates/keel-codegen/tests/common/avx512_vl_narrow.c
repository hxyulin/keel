#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <signal.h>
#include <unistd.h>
@DECLS@
@REFERENCES@
struct Case { const char*name;void(*run)(const void*,void*,void*,uint64_t);void(*reference)(const void*,void*,void*,uint64_t);unsigned width,sb,db,saturation,store,mode; };
static const struct Case cases[]={
@TABLE@
};
static uint64_t read_value(const void*p,unsigned bytes){uint64_t value=0;memcpy(&value,p,bytes);return value;}
static uint64_t convert(const struct Case*t,uint64_t raw){
 const uint64_t cap=(UINT64_C(1)<<t->db)-1;
 if(t->saturation==0)return raw&cap;
 if(t->saturation==2)return raw>cap?cap:raw;
 int64_t value;if(t->sb==32)value=(int64_t)(int32_t)(uint32_t)raw;else memcpy(&value,&raw,8);
 int64_t minimum=-(INT64_C(1)<<(t->db-1)),maximum=(INT64_C(1)<<(t->db-1))-1;
 return (uint64_t)(value<minimum?minimum:value>maximum?maximum:value)&cap;
}
static int scalar(const struct Case*t,const unsigned char*m,const unsigned char*a,const unsigned char*out,uint64_t k){
 unsigned lanes=t->width/t->sb,bytes=t->db/8;
 for(unsigned lane=0;lane<16/bytes;lane++){
  uint64_t expected=0;
  if(lane<lanes){
   if(t->mode==3||((k>>lane)&1))expected=convert(t,read_value(a+lane*t->sb/8,t->sb/8));
   else if(t->mode!=2)expected=read_value(m+lane*bytes,bytes);
  }
  uint64_t actual=read_value(out+lane*bytes,bytes);
  if(actual!=expected){fprintf(stderr,"%s lane %u mask %llx got %llx expected %llx\n",t->name,lane,(unsigned long long)k,(unsigned long long)actual,(unsigned long long)expected);return 1;}
 }
 return 0;
}
static void inputs(const struct Case*t,unsigned char*m,unsigned char*a,unsigned sample){
 uint64_t unsigned_cap=(UINT64_C(1)<<t->db)-1,signed_cap=(UINT64_C(1)<<(t->db-1))-1;
 uint64_t values[]={0,signed_cap,signed_cap+1,(uint64_t)(-(int64_t)signed_cap-1),(uint64_t)(-(int64_t)signed_cap-2),unsigned_cap,unsigned_cap+1,UINT64_MAX};
 for(unsigned i=0;i<64;i++){m[i]=(unsigned char)(0xb7+i*13);a[i]=(unsigned char)(0x31+i*7);}
 for(unsigned i=0;i<t->width/t->sb;i++){
  uint64_t value=values[(sample+i)%8];memcpy(a+i*t->sb/8,&value,t->sb/8);
 }
}
static int snapshots(const struct Case*t,const unsigned char*m,const unsigned char*a,const unsigned char*out,uint64_t k){
 uint64_t saved;memcpy(&saved,out+192,8);
 if(saved!=k)return 1;
 if(t->store){if(memcmp(out+64,m,t->width/8))return 2;}
 else if(memcmp(out+64,m,16)||memcmp(out+128,a,t->width/8))return 3;
 if(t->mode==1&&memcmp(out+256,m,64))return 4;
 return 0;
}
static int fault(const struct Case*t,int system){
 pid_t child=fork();if(child<0)return 1;
 if(child==0){unsigned char a[64]={0},out[384]={0};(system?t->reference:t->run)(a,(void*)1,out,1);_exit(0);}
 int status;if(waitpid(child,&status,0)!=child)return 2;
 return !(WIFSIGNALED(status)&&(WTERMSIG(status)==SIGSEGV||WTERMSIG(status)==SIGBUS));
}
int main(void){
 unsigned original=_mm_getcsr();
 for(unsigned c=0;c<sizeof cases/sizeof cases[0];c++){
  const struct Case*t=cases+c;unsigned lanes=t->width/t->sb;uint64_t all=(UINT64_C(1)<<lanes)-1;
  uint64_t masks[]={0,all,1,UINT64_C(1)<<(lanes-1),0xa5,0xff&~all};
  if(t->store)continue;
  for(unsigned sample=0;sample<8;sample++)for(unsigned p=0;p<6;p++){
   unsigned char m[64],a[64],out[384],reference[16];inputs(t,m,a,sample);memset(out,0xa5,sizeof out);
   const unsigned char*input=t->mode==1?m:a;unsigned mxcsr=(original&~0xe07fu)|((sample&1)?0xe061u:0x0001u);
   _mm_setcsr(mxcsr);t->run(m,a,out,masks[p]);unsigned after=_mm_getcsr();t->reference(m,a,reference,masks[p]);
   if(after!=mxcsr||_mm_getcsr()!=mxcsr){fprintf(stderr,"MXCSR %s\n",t->name);return 1;}
   if(memcmp(out,reference,16)){fprintf(stderr,"system %s sample %u mask %llx\n",t->name,sample,(unsigned long long)masks[p]);return 2;}
   if(scalar(t,m,input,out,masks[p]))return 3;
   if(snapshots(t,m,input,out,masks[p])){fprintf(stderr,"snapshot %s\n",t->name);return 4;}
   if(out[16]!=0xa5||out[200]!=0xa5||out[320]!=0xa5)return 5;
  }
 }
 long page=sysconf(_SC_PAGESIZE);unsigned char*map=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0),*refmap=mmap(0,page*3,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(map==MAP_FAILED||refmap==MAP_FAILED||mprotect(map+page,page,PROT_READ|PROT_WRITE)||mprotect(refmap+page,page,PROT_READ|PROT_WRITE))return 6;
 unsigned char*valid=map+page,*refvalid=refmap+page;
 for(unsigned c=0;c<sizeof cases/sizeof cases[0];c++){
  const struct Case*t=cases+c;if(!t->store)continue;unsigned lanes=t->width/t->sb,bytes=t->db/8,produced=lanes*bytes;
  uint64_t all=(UINT64_C(1)<<lanes)-1,masks[]={0,0xff&~all,1,UINT64_C(1)<<(lanes-1),all,all&0xa5};
  for(unsigned sample=0;sample<8;sample++)for(unsigned p=0;p<6;p++){
   unsigned char m[64],a[64],out[384],reference[16];inputs(t,m,a,sample);memcpy(m,a,64);memset(out,0xa5,sizeof out);memset(valid,0xa5,page);memset(refvalid,0xa5,page);
   unsigned char*pointer=p<2?(unsigned char*)1:p==2?valid+page-bytes:p==3?valid-(lanes-1)*bytes:valid+page-produced;
   unsigned char*refpointer=p<2?(unsigned char*)1:p==2?refvalid+page-bytes:p==3?refvalid-(lanes-1)*bytes:refvalid+page-produced;
   unsigned mxcsr=original|0x3f;_mm_setcsr(mxcsr);t->run(m,pointer,out,masks[p]);t->reference(m,refpointer,reference,masks[p]);
   if(_mm_getcsr()!=mxcsr||memcmp(valid,refvalid,page)||snapshots(t,m,a,out,masks[p])){fprintf(stderr,"store system %s path %u\n",t->name,p);return 7;}
   for(unsigned i=0;i<lanes;i++)if((masks[p]>>i)&1){uint64_t expected=convert(t,read_value(m+i*t->sb/8,t->sb/8));if(read_value(pointer+i*bytes,bytes)!=expected)return 8;}
  }
  // The complete source SSA value must survive a destination overlapping it.
  unsigned char actual[64],expected[64],before[64],a[64],out[384],reference[16];inputs(t,before,a,1);memcpy(actual,before,64);memcpy(expected,before,64);
  t->run(actual,actual+1,out,all);t->reference(before,expected+1,reference,all);
  if(memcmp(actual,expected,64)||memcmp(out+64,before,t->width/8))return 9;
  if(fault(t,0)||fault(t,1)){fprintf(stderr,"active invalid store did not fault %s\n",t->name);return 10;}
 }
 _mm_setcsr(original);return 0;
}
