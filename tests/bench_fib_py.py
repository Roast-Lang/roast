# Benchmark: Fibonacci computation (Python)
# Computes fib(40) which is computationally intensive

import time

def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

start = time.time()
result = fib(40)
end = time.time()

print(f"fib(40) = {result}")
print(f"Time: {end - start:.3f} seconds")
