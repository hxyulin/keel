extern int printf(const char *, ...);

long dimensions(int n, int m) {
    int a[n][m];
    int b[2][n][3];
    int c[n][2][m];
    for (int i=0; i<n; ++i)
        for (int j=0; j<m; ++j) a[i][j]=100*i+j;
    b[1][n-1][2]=17;
    c[n-1][1][m-1]=29;
    long result=a[n-1][m-1]+b[1][n-1][2]+c[n-1][1][m-1];
    result+=sizeof a+sizeof a[0]+sizeof b+sizeof b[0]+sizeof c;
    n=1; m=1;
    return result+sizeof a+sizeof b+sizeof c;
}

long strides(int n, int m) {
    int a[n][m];
    for(int i=0;i<n;i++) for(int j=0;j<m;j++) a[i][j]=i*10+j;
    int (*p)[m]=a;
    int (*q)[m]=p++;
    long result=(*q)[1]+(*p)[1];
    result+=(*++p)[1];
    p-=2; p+=n-1;
    result+=(*p)[m-1]+(p-a)+(a-p);
    q=p--; result+=(*q)[0]+(*p)[0];
    q=--p; result+=(*q)[0];
    int (**pp)[m]=&p;
    *pp+=2; result+=(*p)[1];
    int (*whole)[n][m]=&a;
    result+=sizeof *whole+(whole+1-whole);
    const int (*cp)[m]=a;
    volatile int (*vp)[m]=a;
    result+=cp[1][1]+vp[2][1];
    m=1;
    result+=sizeof *p+(p-a);
    return result;
}

long typedefs(int n, int m) {
    int original=n;
    typedef int Row[n++];
    typedef Row Matrix[m++];
    typedef int (*Pointer)[n++];
    unsigned long row_size=sizeof(Row), matrix_size=sizeof(Matrix);
    n+=20; m+=30;
    Row a, b;
    Matrix matrix;
    a[original-1]=19; b[original-1]=23; matrix[0][original-1]=31;
    Row *p=&a;
    Pointer nil=(Pointer)0;
    long result=sizeof a+sizeof b+sizeof matrix+sizeof *p+sizeof nil;
    result+=a[original-1]+b[original-1]+matrix[0][original-1];
    { typedef char Row[m]; Row inner; inner[m-1]=7; result+=sizeof inner+inner[m-1]; }
    return result+sizeof(Row)+sizeof(Matrix)+row_size+matrix_size+n+m;
}

long pointer_arrays(int n, int m) {
    int a[n][m]; a[1][1]=13; a[2][1]=17;
    int (*table[2])[m]={a,a+1};
    int (*dynamic[n])[m]; dynamic[0]=a+2;
    int i=0;
    long result=sizeof table+sizeof table[i++]+sizeof dynamic;
    result+=i+table[1][0][1]+dynamic[0][0][1];
    return result;
}

long side_effects(int n, int m) {
    int a[n][m]; int b[n][3];
    int (*p)[m]=a;
    int i=0;
    long result=sizeof *p++;
    result+=sizeof a[i++];
    result+=sizeof b[i++]; // fixed-size operand: no increment
    result+=sizeof(p++); // pointer operand: no increment
    result+=i*100+(p-a)*1000;
    int old_n=n, old_m=m;
    result+=sizeof(int[n++][m++]);
    result+=10000*(n-old_n)+100000*(m-old_m);
    int backing[2][m];
    int (*casted)[m]=(int (*)[m++])backing;
    result+=sizeof *casted+(casted+1-casted)+m;
    // Alignment and enclosing unevaluated sizeof discard type-name effects.
    result+=_Alignof(int[n++][m++]);
    result+=sizeof(sizeof(int[n++][m++]));
    result+=n*7+m*11;
    return result;
}

long scope_order(int n) {
    long result=0;
    // A declarator's own identifier is not in scope within its bounds.
    { int n[n]; n[0]=11; result+=sizeof n+n[0]; }
    // A later declarator sees the earlier one, after its bound executes.
    int a[n++], b[sizeof a];
    result+=sizeof a+sizeof b+n;
    for(int j=1;j<20;j++) {
        typedef long Row[j+1];
        Row matrix[2]; matrix[1][j]=j;
        if(j%3==0) continue;
        result+=sizeof matrix+matrix[1][j];
    }
    return result;
}

long capture_only(int n) {
    typedef int Row[n++];
    int (*p)[n++]=0;
    return sizeof(Row)+sizeof p+n;
}

long nested_types(int n, int m) {
    int a[2][4*n];
    int first=n, second=m;
    int (*p)[sizeof(int[n++])]=(int (*)[sizeof(int[(m++,n-1)])])a;
    // The two nested sizeof type names evaluate their own bounds once.
    long result=sizeof *p+n*100+m*1000;
    result+=sizeof *(int (*)[first++])a;
    result+=sizeof(int[sizeof(int[second++])]);
    result+=first*10000+second*100000;
    return result;
}

long optional_bounds(int n, int m) {
    int first=n, second=m;
    long size=sizeof(int (*[n++])[m++]);
    // n affects sizeof and must run; m is permitted to run or be omitted.
    return size==sizeof(void*)*first && n==first+1 && (m==second || m==second+1);
}

long matrix_stress(int n) {
    long total=0;
    for(int j=0;j<20000;j++) {
        typedef volatile char Row[n];
        Row a[2]; a[1][n-1]=j%100;
        { Row b[2]; b[1][n-1]=1; if(j%2) continue; total+=b[1][n-1]; }
        total+=a[1][n-1];
    }
    return total;
}

// Captures inside expressions must execute at the expression's control-flow site.
long conditional_captures(int n) {
    int original=n;
    long total=0;
    for(int i=0;i<6;i++) {
        total += i%2 ? sizeof(int[n++][2]) : sizeof(int[3][n++]);
        0 && sizeof(int[n++]);
        1 || sizeof(int[n++]);
    }
    typedef int Row[n++];
    typedef Row Alias;
    Alias rows[2];
    int (*p)[n++]=(total+=n, (void*)0);
    // The initializer observes the increment performed by its declarator.
    return total+sizeof rows+sizeof *p+sizeof(Alias)+n-original;
}

// Exercise VM layout alongside the independently implemented GNU vector types.
long vector_rows(int n) {
    typedef int V __attribute__((vector_size(16)));
    V a[2][n];
    V v={1,2,3,4};
    a[1][n-1]=v;
    V (*p)[n]=a;
    p++;
    return (*p)[n-1][2]+sizeof *p+sizeof a+(p-a);
}

long review_bounds(int n) {
    long total=0;
    int backing[32][32];
    for(int k=0;k<4;k++) {
        int start=n;
        total+=sizeof(int[sizeof *(int (*)[n++])backing]);
        total+=sizeof *(int (*)[sizeof(int[n++])])backing;
        typedef int Row[n++];
        total+=sizeof *(k%2 ? (Row *)backing : (Row *)backing);
        total+=n-start;
        total+=sizeof ((int (*)[n++])backing);
        total+=sizeof(sizeof *(int (*)[n++])backing);
        total+=_Alignof(*(int (*)[n++])backing);
        total+=0 && sizeof *(int (*)[n++])backing;
        total+=1 || sizeof *(int (*)[n++])backing;
        total+=n;
    }
    return total;
}

long review_pointer_places(int n) {
    int a[5][n];
    for(int i=0;i<5;i++) for(int j=0;j<n;j++) a[i][j]=i*100+j;
    typedef int Row[n];
    Row *volatile table[2]={a,a+1};
    int i=0, j=0;
    Row *old=table[i++]++;
    long total=old[0][n-1]+table[0][0][n-1]+i;
    Row *now=(table[j++]+=2);
    total+=now[0][n-1]+j;
    Row *const fixed=a;
    Row *const *pfixed=&fixed;
    const Row *read=a;
    read++;
    total+=(*pfixed)[0][0]+(*read)[n-1]+sizeof pfixed;
    return total+(table[0]-a);
}

int main(void) {
    for(int n=4;n<10;n++) for(int m=2;m<7;m++)
        printf("%ld %ld %ld %ld %ld %ld\n",dimensions(n,m),strides(n,m),typedefs(n,m),pointer_arrays(n,m),side_effects(n,m),scope_order(n));
    for(int n=4;n<10;n++) printf("%ld %ld %ld\n",capture_only(n),nested_types(n,n+1),optional_bounds(n,n+1));
    printf("%ld %ld %ld\n",matrix_stress(2048),conditional_captures(4),vector_rows(5));
    printf("%ld %ld\n",review_bounds(4),review_pointer_places(4));
    return 0;
}
