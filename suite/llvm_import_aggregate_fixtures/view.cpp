#include <string_view>
#include <cstdint>
using Callback = void* (*)(void*, std::string_view);
extern "C" void* view_helper(void*, std::string_view);
extern "C" void* view_test(void* state, std::string_view view, Callback callback) {
    auto words = static_cast<uint64_t*>(state);
    words[0] = view.size();
    words[1] = view.empty() ? 0 : static_cast<unsigned char>(view[0]);
    void* first = view_helper(state, view);
    return callback(first, view);
}
