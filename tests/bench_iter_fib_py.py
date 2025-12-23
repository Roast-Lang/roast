# Benchmark: Iterative Fibonacci (Python)
# Computes fib(90) using iterative approach

import time

def fib(n):
    if n <= 1:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b

# Warmup
fib(50)

start = time.time()
# Run multiple times since it's too fast
for _ in range(1000000):
    result = fib(90)
end = time.time()

print(f"fib(90) = {result}")
print(f"Time for 1M iterations: {end - start:.3f} seconds")
