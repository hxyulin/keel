using U = unsigned long long;
inline U linkage_state(U x) { static U counter; counter += x; return counter; }
