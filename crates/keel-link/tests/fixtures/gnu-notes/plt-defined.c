long prop(void) { return 99; }
long other = 1;
long *slot = &other;
long call_prop(void) { return prop(); }
