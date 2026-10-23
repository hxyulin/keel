extern int fork(void);extern int waitpid(int,int*,int);extern void _exit(int);
static unsigned long calls,faults;
static void address(long long index,unsigned w,long long*q,unsigned*bit){
 *q=index/(long long)w;long long r=index%(long long)w;if(r<0){--*q;r+=w;}*bit=(unsigned)r;
}
static unsigned byte_reference(const struct IndexedCase*c,const unsigned char*before,unsigned bit,unsigned char*after){
 unsigned n=c->w/8;memcpy(after,before,n);unsigned j=bit/8,m=1u<<(bit%8),result=(before[j]>>(bit%8))&1;
 switch(c->kind){case 0:break;case 1:after[j]|=m;break;case 2:after[j]&=(unsigned char)~m;break;case 3:after[j]^=m;break;default:__builtin_trap();}return result;
}
static int fault(const struct IndexedCase*c,void*base,void*index,void*out){
 int pid=fork(),status=0;if(pid<0)return 30;if(!pid){unsigned char w[64];c->keel(base,index,out,w);_exit(0);}if(waitpid(pid,&status,0)!=pid)return 31;unsigned sig=status&127;if(sig!=11&&sig!=7){printf("FAIL missing indexed fault %s status=%d\n",c->name,status);return 32;}faults++;return 0;
}
int main(void){
 unsigned long page=(unsigned long)getpagesize();unsigned char*maps[4];for(unsigned i=0;i<4;i++){maps[i]=mmap(0,page*3,0,2|32,-1,0);if(maps[i]==(void*)-1||mprotect(maps[i]+page,page,3))return 20;}
 unsigned long expected_calls=0;
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){
  const struct IndexedCase*c=&cases[ci];unsigned n=c->w/8;expected_calls+=(2*c->w+8)*(4*c->w+3)*4+2;
  for(unsigned pattern=0;pattern<2*c->w+8;pattern++)for(long long index=-(long long)(2*c->w)-1;index<=(long long)(2*c->w)+1;index++)for(unsigned alias=0;alias<4;alias++){
   unsigned char aa[512],ib[96],oo[96],ea[512],ei[96],eo[96],original[8],encoded[8],modified[8],hw[128],eh[128],w[64],want[64],flag;
   memset(aa,0xa5,512);memset(ib,0xa5,96);memset(oo,0xa5,96);unsigned off=pattern%32;unsigned char*selected=aa+192+off,*idx=ib+32+off;
   for(unsigned j=0;j<n;j++){
    unsigned pos=pattern%c->w;unsigned char bit=(unsigned char)(j==pos/8?1u<<(pos%8):0);
    selected[j]=pattern<c->w?bit:pattern<2*c->w?(unsigned char)~bit:(unsigned char)(pattern*37+j*59);
   }
   unsigned long long raw=(unsigned long long)index;memcpy(idx,&raw,n);if(alias==3){idx=selected;memcpy(idx,&raw,n);}memcpy(encoded,idx,n);memcpy(original,selected,n);
   long long q;unsigned bit;address(index,c->w,&q,&bit);unsigned char*base=selected-q*(long long)n,*out=alias==1?selected:alias==2?idx:oo+32+off;
   unsigned expected=byte_reference(c,original,bit,modified);memcpy(ea,aa,512);memcpy(ei,ib,96);memcpy(eo,oo,96);memcpy(ea+192+off,modified,n);
   if(alias==1)ea[192+off]=(unsigned char)expected;else if(alias==2)ei[32+off]=(unsigned char)expected;else eo[32+off]=(unsigned char)expected;
   memset(hw,0xa5,128);memcpy(hw+64,original,n);memcpy(eh,hw,128);memcpy(eh+64,modified,n);unsigned state=csr();c->hardware(hw+64-q*(long long)n,encoded,&flag);
   if(flag!=expected||memcmp(hw,eh,128)||csr()!=state){printf("FAIL indexed hardware %s index=%lld\n",c->name,index);return 10;}
   memset(w,0x79,64);memset(want,0x79,64);memcpy(want,original,n);memcpy(want+32,encoded,n);c->keel(base,idx,out,w);calls++;
   if(memcmp(aa,ea,512)||memcmp(ib,ei,96)||memcmp(oo,eo,96)||memcmp(w,want,64)||csr()!=state){printf("FAIL indexed recipe %s index=%lld pattern=%u alias=%u\n",c->name,index,pattern,alias);return 11;}
  }
  for(unsigned side=0;side<2;side++){
   for(unsigned i=0;i<4;i++)if(mprotect(maps[i]+page,page,3))return 21;
   unsigned char*selected=maps[0]+page+(side?page-n:0),*idx=maps[1]+page+(side?page-n:0),*out=maps[2]+page+(side?page-1:0),*hselected=maps[3]+page+(side?page-n:0),original[8],encoded[8],modified[8],w[64],want[64],hardware;
   long long index=side?(long long)c->w+1:-(long long)c->w-1,q;unsigned bit;address(index,c->w,&q,&bit);unsigned long long raw=(unsigned long long)index;memcpy(idx,&raw,n);memcpy(encoded,idx,n);
   for(unsigned j=0;j<n;j++)selected[j]=(unsigned char)(j*37+131);memcpy(original,selected,n);memcpy(hselected,original,n);unsigned expected=byte_reference(c,original,bit,modified);
   if(mprotect(maps[0]+page,page,c->kind?3:1)||mprotect(maps[1]+page,page,1)||mprotect(maps[3]+page,page,c->kind?3:1))return 22;
   unsigned state=csr();c->hardware(hselected-q*(long long)n,idx,&hardware);if(hardware!=expected||memcmp(hselected,modified,n)||csr()!=state)return 23;
   memset(w,0x79,64);memset(want,0x79,64);memcpy(want,original,n);memcpy(want+32,encoded,n);c->keel(selected-q*(long long)n,idx,out,w);calls++;
   if(*out!=expected||memcmp(selected,modified,n)||memcmp(w,want,64)||csr()!=state)return 24;
   long delta=side?1:-1;
   if(fault(c,selected-q*(long long)n+delta,idx,out)||fault(c,selected-q*(long long)n,idx+delta,out)||fault(c,selected-q*(long long)n,idx,out+delta))return 25;
  }
 }
 for(unsigned i=0;i<4;i++)if(munmap(maps[i],page*3))return 26;
 if(calls!=expected_calls||faults!=72)return 40;
 printf("PASS signed-bit-index cases=12 calls=%lu faults=%lu hardware-and-byte-index\n",calls,faults);return 0;
}
