extern unsigned long goto_rw_output(int,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long,unsigned long);
extern int goto_tied_output(int,int),goto_numeric_rw_output(int,int),goto_memory_outputs(int,int*,int),goto_flag_output(int),goto_backward_output(int),goto_output_address(int,int*);
int main(void) {
    for(int branch=-1;branch<=1;++branch) {
        unsigned long live=2+3*3+5*5+7*7+11*11+13*13+17*17+19*19;
        if(goto_rw_output(branch,0xfffffff000000003UL,2,3,5,7,11,13,17,19)!=0xfffffff000000003UL+7+live+(branch?103:101))return 1;
        if(goto_tied_output(branch,0x12345000)!=0x12345000+11+(branch?17:13))return 2;
        if(goto_numeric_rw_output(branch,0x12345000)!=0x12345000+19+(branch?29:23))return 3;
        int canaries[]={0x12345678,73,0x56781234};
        if(goto_memory_outputs(branch,canaries+1,79)!=73+31+79+37+(branch?43:41))return 4;
        if(canaries[0]!=0x12345678||canaries[1]!=104||canaries[2]!=0x56781234)return 5;
        if(goto_flag_output(branch)!=47+!branch+(branch?59:53))return 6;
        if(goto_output_address(branch,canaries+1)!=61+1+(branch?71:67))return 7;
        if(canaries[0]!=0x12345678||canaries[1]!=61||canaries[2]!=0x56781234)return 8;
    }
    for(int n=1;n<7;++n)if(goto_backward_output(n))return 9;
    return 0;
}
