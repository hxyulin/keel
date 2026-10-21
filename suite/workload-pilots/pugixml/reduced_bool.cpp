// Independently authored reproducer of the first pugixml signature blocker.
// This is not an edited subset of upstream source or a workload substitute.
bool pilot_bool(unsigned value) { return value != 0; }
int main() { return pilot_bool(0) || !pilot_bool(1); }
