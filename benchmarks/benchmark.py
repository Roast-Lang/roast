# Benchmark: Python vs Roast
# Tests: Fibonacci, Prime counting, Matrix multiplication

import time

def fib_recursive(n):
    """Recursive fibonacci - O(2^n)"""
    if n <= 1:
        return n
    return fib_recursive(n - 1) + fib_recursive(n - 2)

def fib_iterative(n):
    """Iterative fibonacci - O(n)"""
    if n <= 1:
        return n
    a, b = 0, 1
    for i in range(2, n + 1):
        a, b = b, a + b
    return b

def is_prime(n):
    """Check if n is prime"""
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    i = 3
    while i * i <= n:
        if n % i == 0:
            return False
        i += 2
    return True

def count_primes(limit):
    """Count primes up to limit"""
    count = 0
    for n in range(2, limit + 1):
        if is_prime(n):
            count += 1
    return count

def sum_loop(n):
    """Simple loop sum - tests loop performance"""
    total = 0
    for i in range(1, n + 1):
        total += i
    return total

if __name__ == "__main__":
    print("=" * 50)
    print("PYTHON BENCHMARK")
    print("=" * 50)
    
    # Test 1: Recursive Fibonacci
    start = time.time()
    result = fib_recursive(35)
    elapsed = time.time() - start
    print(f"1. fib_recursive(35) = {result}")
    print(f"   Time: {elapsed:.3f}s")
    
    # Test 2: Iterative Fibonacci
    start = time.time()
    for _ in range(1000):
        result = fib_iterative(1000)
    elapsed = time.time() - start
    print(f"2. fib_iterative(1000) x 1000 iterations")
    print(f"   Time: {elapsed:.3f}s")
    
    # Test 3: Prime counting
    start = time.time()
    result = count_primes(100000)
    elapsed = time.time() - start
    print(f"3. count_primes(100000) = {result}")
    print(f"   Time: {elapsed:.3f}s")
    
    # Test 4: Loop sum
    start = time.time()
    result = sum_loop(10000000)
    elapsed = time.time() - start
    print(f"4. sum_loop(10_000_000) = {result}")
    print(f"   Time: {elapsed:.3f}s")
    
    print("=" * 50)
