int main(void){
 for(unsigned c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
  const struct Case*t=&cases[c];unsigned lane_bytes=t->bits/8;uint64_t mask=(1ull<<t->bits)-1,sign=1ull<<(t->bits-1);
  uint64_t edges[]={0,1,2,sign-1,sign,sign+1,mask-1,mask};
  const unsigned char selectors[]={0,1,127,128,129,255,85,170};
  for(unsigned test=0;test<64+256;test++){
   unsigned char a[40],b[40],m[40],actual[40],reference[40],expected[40];
   for(unsigned i=0;i<40;i++)a[i]=b[i]=m[i]=actual[i]=reference[i]=expected[i]=0xa5;
   for(unsigned lane=0;lane<t->bytes/lane_bytes;lane++){
    uint64_t x=test<64?edges[(test/8+lane)%8]:random_word()&mask;
    uint64_t y=test<64?edges[(test%8+3*lane)%8]:random_word()&mask;
    write_lane(a+1+lane*lane_bytes,lane_bytes,x);write_lane(b+1+lane*lane_bytes,lane_bytes,y);
   }
   for(unsigned i=0;i<t->bytes;i++)m[i+1]=selectors[(test+i)%8];
   t->keel(a+1,b+1,m+1,actual+1);t->vendor(a+1,b+1,m+1,reference+1);
   for(unsigned lane=0;lane<t->bytes/lane_bytes;lane++){
    unsigned at=1+lane*lane_bytes;uint64_t x=read_lane(a+at,lane_bytes),y=read_lane(b+at,lane_bytes),want;
    int64_t sx=signed_lane(x,t->bits),sy=signed_lane(y,t->bits);
    switch(t->operation){
     case 0:want=(t->signed_lanes?sx<sy:x<y)?x:y;break;
     case 1:want=(t->signed_lanes?sx>sy:x>y)?x:y;break;
     case 2:want=sx<0?(0-x)&mask:x;break;
     case 3:want=sy<0?(0-x)&mask:sy==0?0:x;break;
     default:want=m[at]&128?y:x;break;
    }
    write_lane(expected+at,lane_bytes,want);
   }
   for(unsigned i=0;i<40;i++)if(actual[i]!=expected[i]||actual[i]!=reference[i]){printf("%s test %u byte %u: keel=%u vendor=%u scalar=%u\n",t->name,test,i,actual[i],reference[i],expected[i]);return 1;}
  }
 }
 return 0;
}
