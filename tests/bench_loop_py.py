# Benchmark: Loop computation (Python)
# Sum loop with modulo operation - 10 million iterations

import time

def benchmark():
    total = 0
    for i in range(10000000):
        total = total + (i % 7)
    return total

start = time.time()
result = benchmark()
end = time.time()

print(f"Result = {result}")
print(f"Time: {end - start:.3f} seconds")
