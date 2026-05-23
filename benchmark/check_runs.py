def run_nn(L, N):
    s = 0
    for l in range(L):
        t1 = l * 17
        for n in range(N):
            t2 = n * 31
            val = t1 + t2
            weight = val % 10
            bias = (l + n) % 5
            act = weight * n + bias
            if act > 5:
                s += act
    return s

print("L=10000, N=10000:", run_nn(10000, 10000))
print("L=100000, N=1000:", run_nn(100000, 1000))
print("L=1000, N=100000:", run_nn(1000, 100000))
