#include <iostream>

int main() {
    long long sum = 0;
    long long layers = 10000000;
    long long nodes = 100000;
    
    long long l = 0;
    while (l < layers) {
        long long t1 = l * 17;
        long long n = 0;
        while (n < nodes) {
            long long t2 = n * 31;
            long long val = t1 + t2;
            
            long long div = val / 10;
            long long term = div * 10;
            long long weight = val - term;
            
            long long val2 = l + n;
            long long div2 = val2 / 5;
            long long term2 = div2 * 5;
            long long bias = val2 - term2;
            
            long long t3 = weight * n;
            long long act = t3 + bias;
            if (act > 5) {
                sum += act;
            }
            n++;
        }
        l++;
    }
    std::cout << sum << std::endl;
    return 0;
}
