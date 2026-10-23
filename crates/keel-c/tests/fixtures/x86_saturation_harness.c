/* Independent widened scalar clamp and system vendor-header oracles. */
extern int eval_count;
__m128i callback(__m128i a, __m128i b) { return _mm_xor_si128(a,b); }
static __m128i load(const void *p) { __m128i v; memcpy(&v,p,16); return v; }
static void scalar(int op,const unsigned char *a,const unsigned char *b,unsigned char *out) {
    int word=op&1, uns=(op&2)!=0, sub=op>=4;
    for(int i=0;i<16;i+=word?2:1) {
        int x,y;
        if(word) {
            uint16_t ux,uy; memcpy(&ux,a+i,2); memcpy(&uy,b+i,2);
            x=ux; y=uy;
            if(!uns) { if(x>=32768)x-=65536; if(y>=32768)y-=65536; }
        } else {
            x=a[i]; y=b[i];
            if(!uns) { if(x>=128)x-=256; if(y>=128)y-=256; }
        }
        int z=sub?x-y:x+y, lo=uns?0:(word?-32768:-128), hi=uns?(word?65535:255):(word?32767:127);
        if(z<lo)z=lo; if(z>hi)z=hi;
        if(word) { uint16_t u=(uint16_t)z; memcpy(out+i,&u,2); }
        else out[i]=(unsigned char)z;
    }
}
static int equal(const void*a,const void*b,size_t n,const char *what,int op) {
    if(!memcmp(a,b,n))return 1;
    fprintf(stderr,"saturation %s op=%d\n",what,op); return 0;
}
static int values(int op,const unsigned char*a,const unsigned char*b) {
    unsigned char expected[16],got[16],oracle[16]; scalar(op,a,b,expected);
    __m128i av=load(a),bv=load(b),z=sat[op](av,bv),v=vendor[op](av,bv);
    memcpy(got,&z,16);memcpy(oracle,&v,16);
    return equal(expected,got,16,"scalar",op)&&equal(oracle,got,16,"vendor",op);
}
static int transport(int op,const unsigned char*a,const unsigned char*b) {
    unsigned char out[80],expected[16],copy[32],cb[16];
    scalar(op,a,b,expected); memset(out,0xa5,sizeof(out)); eval_count=0;
    mem[op](a,b,out+1);
    if(eval_count!=2||out[0]!=0xa5||!equal(out+1,expected,16,"unaligned",op))return 0;
    for(int i=17;i<80;i++)if(out[i]!=0xa5)return 0;
    memset(out,0xa5,sizeof(out)); eval_count=0; live[op](a,b,out+1);
    for(int i=0;i<16;i++)cb[i]=a[i]^b[i];
    if(eval_count!=2||out[0]!=0xa5||!equal(out+1,expected,16,"live result",op)||!equal(out+17,a,16,"live a",op)||!equal(out+33,b,16,"live b",op)||!equal(out+49,cb,16,"callback",op))return 0;
    for(int i=65;i<80;i++)if(out[i]!=0xa5)return 0;
    __m128i z=indirect[op](load(a),load(b),callback);memcpy(out,&z,16);scalar(op,a,cb,expected);
    if(!equal(out,expected,16,"indirect/live",op))return 0;
    z=alias[op](load(a));memcpy(out,&z,16);scalar(op,a,a,expected);
    if(!equal(out,expected,16,"same input",op))return 0;
    scalar(op,a,b,expected);
    memcpy(copy,a,16);memcpy(copy+16,b,16);mem[op](copy,copy+16,copy);
    if(!equal(copy,expected,16,"output alias a",op)||!equal(copy+16,b,16,"alias b preserved",op))return 0;
    memcpy(copy,a,16);memcpy(copy+16,b,16);mem[op](copy,copy+16,copy+16);
    return equal(copy+16,expected,16,"output alias b",op)&&equal(copy,a,16,"alias a preserved",op);
}
static unsigned char *guard(size_t n) {
    unsigned char*p=mmap(0,3*n,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if(p==MAP_FAILED||mprotect(p+n,n,PROT_READ|PROT_WRITE))return 0;
    memset(p+n,0xa5,n);return p;
}
int main(void) {
    unsigned char a[16],b[16];
    for(int op=0;op<8;op++) {
        if(!(op&1)) {
            /* Every ordered byte pair in every lane, with lane-varying data. */
            for(int x=0;x<256;x++)for(int y=0;y<256;y++) {
                for(int k=0;k<16;k++){a[k]=(unsigned char)(x+k);b[k]=(unsigned char)(y+17*k);}
                if(!values(op,a,b))return 1;
            }
        } else {
            const uint16_t edge[]={0,1,2,126,127,128,129,254,255,256,257,32766,32767,32768,32769,65533,65534,65535};
            for(unsigned x=0;x<sizeof(edge)/2;x++)for(unsigned y=0;y<sizeof(edge)/2;y++) {
                for(int k=0;k<8;k++){uint16_t u=edge[(x+k)%18],v=edge[(y+3*k)%18];memcpy(a+2*k,&u,2);memcpy(b+2*k,&v,2);}
                if(!values(op,a,b)||!transport(op,a,b))return 2;
            }
            /* Every word bit pattern paired with zero and signed/unsigned limits. */
            for(unsigned x=0;x<65536;x++) {
                for(int k=0;k<8;k++){uint16_t u=(uint16_t)(x+k),v=edge[(x+k)%18];memcpy(a+2*k,&u,2);memcpy(b+2*k,&v,2);}
                if(!values(op,a,b))return 3;
            }
        }
        uint32_t rng=0x4b1dbeef;
        for(int n=0;n<4096;n++) {
            for(int k=0;k<16;k++){rng=rng*1664525u+1013904223u;a[k]=(unsigned char)(rng>>24);rng=rng*1664525u+1013904223u;b[k]=(unsigned char)(rng>>24);}
            if(!values(op,a,b)||!transport(op,a,b))return 4;
        }
        size_t n=(size_t)sysconf(_SC_PAGESIZE);
        unsigned char *pa=guard(n),*pb=guard(n),*po=guard(n);
        if(!pa||!pb||!po)return 5;
        for(int side=0;side<3;side++) {
            size_t offset=side==0?0:(side==1?n-16:1);
            unsigned char *ap=pa+n+offset,*bp=pb+n+offset,*out=po+n+offset,expected[16];
            memcpy(ap,a,16);memcpy(bp,b,16);scalar(op,a,b,expected);
            mem[op](ap,bp,out);
            if(!equal(expected,out,16,"guard",op))return 6;
            for(size_t k=0;k<n;k++)if((k<offset||k>=offset+16)&&po[n+k]!=0xa5)return 7;
            memset(po+n,0xa5,n);
        }
        munmap(pa,3*n);munmap(pb,3*n);munmap(po,3*n);
    }
    puts("SSE2 saturation: scalar/vendor, exhaustive bytes, word edges/random, ABI/live/aliases, guards passed");
    return 0;
}
