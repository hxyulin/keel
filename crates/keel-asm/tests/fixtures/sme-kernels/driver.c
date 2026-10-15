// Native correctness driver. No SME instructions are emitted from this C file.
// Rust checks OS advertisement before this executable is launched.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern void matmul_fp32(const void *,const void *,void *,size_t,size_t,size_t);
extern void matmul_bf16(const void *,const void *,void *,size_t,size_t,size_t);
extern void matmul_int8(const void *,const void *,void *,size_t,size_t,size_t);
extern size_t streaming_lanes(void);
struct buffer { void *mapping; unsigned char *data; size_t size, mapped; };
static struct buffer guarded(size_t bytes) {
    size_t page=(size_t)sysconf(_SC_PAGESIZE);
    size_t accessible=((bytes+32+page-1)/page)*page;
    void *p=mmap(0,accessible+page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
    if(p==MAP_FAILED || mprotect((char *)p+accessible,page,PROT_NONE)) abort();
    struct buffer b={p,(unsigned char*)p+accessible-bytes,bytes,accessible+page};
    memset(b.data-32,0xa5,32);
    return b;
}
static void release(struct buffer b) { munmap(b.mapping,b.mapped); }
static uint16_t bf16(float f) {uint32_t u;memcpy(&u,&f,4);return (uint16_t)(u>>16);}
static float af(size_t i,size_t k) {return ((int)((i*5+k*3)%17)-8)*0.25f;}
static float bf(size_t k,size_t j) {return ((int)((k*7+j*11)%19)-9)*0.25f;}
static int ai(size_t i,size_t k) {return (int)((i*5+k*3)%17)-8;}
static int bi(size_t k,size_t j) {return (int)((k*7+j*11)%19)-9;}
static int test(int type,size_t m,size_t n,size_t k) {
    size_t group=type==0?1:type==1?2:4;
    size_t blocks=(k+group-1)/group;
    struct buffer a=guarded(blocks*m*4), b=guarded(blocks*n*4), c=guarded(m*n*4);
    memset(a.data,0,a.size);memset(b.data,0,b.size);memset(c.data,0x7f,c.size);
    for(size_t kk=0;kk<k;kk++) {
        for(size_t i=0;i<m;i++) {
            size_t off=((kk/group)*m+i)*group+kk%group;
            if(type==0)((float*)a.data)[off]=af(i,kk);
            else if(type==1)((uint16_t*)a.data)[off]=bf16(af(i,kk));
            else ((int8_t*)a.data)[off]=(int8_t)ai(i,kk);
        }
        for(size_t j=0;j<n;j++) {
            size_t off=((kk/group)*n+j)*group+kk%group;
            if(type==0)((float*)b.data)[off]=bf(kk,j);
            else if(type==1)((uint16_t*)b.data)[off]=bf16(bf(kk,j));
            else ((int8_t*)b.data)[off]=(int8_t)bi(kk,j);
        }
    }
    if(type==0)matmul_fp32(a.data,b.data,c.data,m,n,blocks);
    else if(type==1)matmul_bf16(a.data,b.data,c.data,m,n,blocks);
    else matmul_int8(a.data,b.data,c.data,m,n,blocks);
    for(size_t i=0;i<m;i++)for(size_t j=0;j<n;j++) {
        float fp=0;int ip=0;
        for(size_t kk=0;kk<k;kk++) {
            if(type==2)ip+=ai(i,kk)*bi(kk,j);else fp+=af(i,kk)*bf(kk,j);
        }
        if((type==2 && ((int32_t*)c.data)[i*n+j]!=ip) ||
           (type!=2 && ((float*)c.data)[i*n+j]!=fp)) {
            fprintf(stderr,"type=%d M=%zu N=%zu K=%zu at (%zu,%zu): got %g expected %g\n",type,m,n,k,i,j,
                    type==2?(double)((int32_t*)c.data)[i*n+j]:((float*)c.data)[i*n+j],type==2?(double)ip:fp);
            return 1;
        }
    }
    for(int i=1;i<=32;i++)if(c.data[-i]!=0xa5) {fprintf(stderr,"output underrun\n");return 1;}
    release(a);release(b);release(c);return 0;
}
int main(void) {
    size_t vl=streaming_lanes();
    if(vl<4 || vl>64) return 1;
    size_t dims[][2]={{0,vl+1},{vl+1,0},{1,1},{3,5},{vl-1,vl-1},{vl,vl},{vl+1,vl+3},{2*vl+3,2*vl+1}};
    size_t ks[]={0,1,2,3,4,5,7,17,33};
    for(int type=0;type<3;type++)for(size_t d=0;d<sizeof(dims)/sizeof(dims[0]);d++)
        for(size_t t=0;t<sizeof(ks)/sizeof(ks[0]);t++)if(test(type,dims[d][0],dims[d][1],ks[t]))return 1;
    return 0;
}
