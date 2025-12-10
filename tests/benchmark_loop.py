# Benchmark: 1 billion loop iterations with computation (Python)

def benchmark():
    sum = 0
    i = 0
    while i < 1000000000:
        sum = sum + i % 7
        i = i + 1
    print(sum)

benchmark()
