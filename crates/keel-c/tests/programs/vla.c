
long sum(int n) {
    int a[n++];
    unsigned long size = sizeof a;
    for (int i=0; i<n-1; ++i) a[i]=i+3;
    long result=0;
    for (int i=0; i<n-1; ++i) result += a[i];
    { double b[n]; b[n-1]=3; result += (long)b[n-1] + sizeof b; }
    return result + size;
}
long loops(int n) {
    long total=0;
    for (int j=0; j<100; ++j) {
        long a[n+j]; a[0]=j;
        { int b[n]; b[0]=j+2; if (j%3==0) continue; total+=b[0]; }
        if (j==83) break;
        total+=a[0];
    }
    int k=0;
    do { int a[n]; a[0]=++k; if(k<3) continue; total+=a[0]; } while(k<4);
    while(k--) { int a[n]; a[0]=k; if(k==1) break; total+=a[0]; }
    return total;
}
long for_init(int n) {
    long result=0;
    for (int a[n], j=0; j<6; ++j) { a[j%n]=j; if(j==2) continue; result+=a[j%n]; }
    return result;
}
long early(int n) {
    int a[n]; a[n-1]=17;
    { int b[n+1]; b[n]=23; if(n>1) return a[n-1]+b[n]+sizeof a; }
    return a[0];
}
long comma_bound(int n) { int a[(n++,5)]; return n+sizeof a; }
long statement(int n) { return ({ int a[n]; a[0]=n+1; a[0]+sizeof a; }); }

int feed(int *a, int n) { int sum=0; for(int i=0;i<n;i++) sum+=a[i]; return sum; }
long pointers(int n) {
    int a[n+4]; for(int i=0;i<n+4;i++) a[i]=i+1;
    int *p=a; ++p; p+=2;
    return *(p-1)+(a+n-a)+feed(a,n);
}
struct R {long x,y,z;};
struct R aggregate(int n) {
    int a[n]; a[0]=n;
    struct R result={a[0], sizeof a, 29};
    return result;
}
void nothing(int n) { int a[n]; a[0]=n; if(n>3) return; }
void fallthrough(int n) { int a[n]; a[0]=n; }

extern int printf(const char *, ...);
struct Item { long x; double y; };
long objects(int n) {
    struct Item a[n]; a[n-1].x=13; a[n-1].y=2.5;
    char b[n]; b[n-1]=3;
    return a[n-1].x + (long)a[n-1].y + b[n-1] + sizeof a + sizeof b;
}
long stress(int n) {
    long total=0;
    for(int j=0; j<20000; ++j) {
        volatile char a[n]; a[n-1]=j%100;
        { volatile char b[n]; b[n-1]=1; if(j%2) continue; total+=b[n-1]; }
        total+=a[n-1];
    }
    int j=0;
    while(j++<20000) { volatile char a[n]; a[0]=1; total+=a[0]; }
    j=0;
    do { volatile char a[n]; a[0]=1; total+=a[0]; } while(++j<20000);
    for(int j=0; j<20000; ++j) {
        while(1) { volatile char a[n]; a[0]=1; total+=a[0]; break; }
    }
    return total;
}
// A struct value escaping a statement expression must outlive its VLA storage.
struct Box { long values[3]; };
long statement_aggregate(int n) {
    return ({ struct Box a[n]; a[0].values[0]=17; a[0]; }).values[
        ({ volatile long scratch[3*n]; scratch[0]=99; 0; })];
}

typedef int Vec __attribute__((vector_size(16)));
typedef unsigned int Wide __attribute__((mode(DI)));
long numeric_elements(int n) {
    int bound=n;
    Vec a[bound++];
    volatile Vec b[n];
    Wide c[n];
    a[n-1]=(Vec){1,2,3,4};
    int lane=1;
    a[n-1][lane++] += 9;
    b[n-1]=a[n-1];
    Vec copy=b[n-1];
    c[n-1]=0x100000001UL;
    return sizeof a + sizeof b + sizeof c + bound + lane + copy[1] + c[n-1];
}

int main(void) {
    for(int n=2; n<20; ++n) printf("%ld %ld %ld %ld %ld %ld %ld\n",sum(n),loops(n),for_init(n),early(n),statement(n),objects(n),comma_bound(n));
    for(int n=2; n<20; ++n) {
        struct R r=aggregate(n);
        nothing(n); fallthrough(n);
        printf("%ld %ld %ld %ld\n",pointers(n),r.x,r.y,r.z);
    }
    printf("%ld %ld %ld\n",stress(4096),statement_aggregate(4),numeric_elements(4));
    return 0;
}
