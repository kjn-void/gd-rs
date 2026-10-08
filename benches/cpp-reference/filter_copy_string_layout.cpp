// Untimed standard-library representation probe for the filter-copy reports.
#include <cstdint>
#include <iostream>
#include <string>

int main() {
#if defined(_LIBCPP_VERSION)
    std::cout << "libc++ " << _LIBCPP_VERSION << '\n';
#elif defined(__GLIBCXX__)
    std::cout << "libstdc++ " << __GLIBCXX__ << '\n';
#endif
    for (const auto length : {16U, 128U}) {
        const std::string value(length, 'x');
        const auto object = reinterpret_cast<std::uintptr_t>(&value);
        const auto data = reinterpret_cast<std::uintptr_t>(value.data());
        std::cout << "length=" << length << " sizeof_string=" << sizeof(value)
                  << " capacity=" << value.capacity()
                  << " payload_inside_object="
                  << (data >= object && data < object + sizeof(value)) << '\n';
    }
}
