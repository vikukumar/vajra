#include <iostream>

int main() {
    long long sum = 0;
    long long i = 0;
    while (i < 1000000000) {
        sum += i;
        i++;
    }
    std::cout << sum << std::endl;
    return 0;
}
