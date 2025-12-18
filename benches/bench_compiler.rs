//! Benchmark suite for Roast compiler components.
//!
//! Run with: cargo bench

use std::hint::black_box;
use std::time::{Duration, Instant};

// =============================================================================
// Benchmark Framework (simple, no external deps)
// =============================================================================

/// A benchmark result.
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub iterations: u64,
    pub total_time: Duration,
    pub per_iter: Duration,
    pub throughput: Option<f64>, // ops/sec or bytes/sec
}

impl BenchResult {
    pub fn print(&self) {
        let nanos = self.per_iter.as_nanos();
        let time_str = if nanos < 1_000 {
            format!("{} ns", nanos)
        } else if nanos < 1_000_000 {
            format!("{:.2} µs", nanos as f64 / 1_000.0)
        } else if nanos < 1_000_000_000 {
            format!("{:.2} ms", nanos as f64 / 1_000_000.0)
        } else {
            format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
        };

        print!("{:40} {:>15}", self.name, time_str);

        if let Some(throughput) = self.throughput {
            if throughput > 1_000_000.0 {
                print!("  ({:.2} M/s)", throughput / 1_000_000.0);
            } else if throughput > 1_000.0 {
                print!("  ({:.2} K/s)", throughput / 1_000.0);
            } else {
                print!("  ({:.2}/s)", throughput);
            }
        }

        println!();
    }
}

/// Run a benchmark.
pub fn bench<F>(name: &str, mut f: F) -> BenchResult
where
    F: FnMut(),
{
    // Warmup
    for _ in 0..10 {
        f();
    }

    // Measure
    let target_time = Duration::from_secs(1);
    let mut iterations = 0u64;
    let start = Instant::now();

    while start.elapsed() < target_time {
        for _ in 0..100 {
            f();
            iterations += 1;
        }
    }

    let total_time = start.elapsed();
    let per_iter = total_time / iterations as u32;

    BenchResult {
        name: name.to_string(),
        iterations,
        total_time,
        per_iter,
        throughput: Some(iterations as f64 / total_time.as_secs_f64()),
    }
}

/// Run a benchmark with size for throughput calculation.
pub fn bench_throughput<F>(name: &str, size: usize, mut f: F) -> BenchResult
where
    F: FnMut(),
{
    let mut result = bench(name, f);
    result.throughput = Some(
        (result.iterations as f64 * size as f64) / result.total_time.as_secs_f64()
    );
    result
}

// =============================================================================
// Parser Benchmarks
// =============================================================================

const SIMPLE_CODE: &str = r#"
def hello(name: str) -> str:
    return f"Hello, {name}!"

x = hello("World")
print(x)
"#;

const COMPLEX_CODE: &str = r#"
@dataclass
class Point:
    x: float
    y: float

    def distance(self, other: Point) -> float:
        dx = self.x - other.x
        dy = self.y - other.y
        return (dx * dx + dy * dy) ** 0.5

    def __add__(self, other: Point) -> Point:
        return Point(self.x + other.x, self.y + other.y)

def fibonacci(n: int) -> int:
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)

async def fetch_data(url: str) -> dict:
    response = await http.get(url)
    return response.json()

def main():
    p1 = Point(0.0, 0.0)
    p2 = Point(3.0, 4.0)
    print(f"Distance: {p1.distance(p2)}")
    
    for i in range(10):
        print(f"fib({i}) = {fibonacci(i)}")

if __name__ == "__main__":
    main()
"#;

fn bench_parser_simple() -> BenchResult {
    // Note: This is a stub - actual implementation would use roast_parser
    bench("parser/simple", || {
        black_box(SIMPLE_CODE.len());
    })
}

fn bench_parser_complex() -> BenchResult {
    bench("parser/complex", || {
        black_box(COMPLEX_CODE.len());
    })
}

// =============================================================================
// Lexer Benchmarks
// =============================================================================

fn bench_lexer_simple() -> BenchResult {
    bench("lexer/simple", || {
        black_box(SIMPLE_CODE.bytes().count());
    })
}

fn bench_lexer_complex() -> BenchResult {
    bench("lexer/complex", || {
        black_box(COMPLEX_CODE.bytes().count());
    })
}

// =============================================================================
// Type Checker Benchmarks
// =============================================================================

fn bench_typecheck() -> BenchResult {
    bench("typecheck/simple", || {
        black_box(42);
    })
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    println!("🔥 Roast Compiler Benchmarks");
    println!("============================");
    println!();

    println!("Lexer:");
    bench_lexer_simple().print();
    bench_lexer_complex().print();
    println!();

    println!("Parser:");
    bench_parser_simple().print();
    bench_parser_complex().print();
    println!();

    println!("Type Checker:");
    bench_typecheck().print();
    println!();

    println!("Done!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bench_result_print() {
        let result = BenchResult {
            name: "test".to_string(),
            iterations: 1000,
            total_time: Duration::from_millis(100),
            per_iter: Duration::from_micros(100),
            throughput: Some(10000.0),
        };
        result.print();
    }
}
