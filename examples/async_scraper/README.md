# Async Scraper - Multi-Threaded Roast Application

A complex, multi-file web scraper demonstrating Roast's advanced features.

## Features

- 🔄 **Async/Await** - Non-blocking I/O operations
- 📁 **Multi-File Structure** - Modular code organization
- 🧵 **Multi-Threading** - Concurrent web requests
- 📦 **Classes** - OOP with type annotations
- 🔌 **Imports** - Cross-module dependencies

## Project Structure

```
async_scraper/
├── roast.toml              # Project configuration
├── README.md
├── src/
│   ├── main.roast          # Entry point - coordinates scraping
│   ├── http/
│   │   └── client.roast    # Async HTTP client
│   ├── storage/
│   │   └── store.roast     # Data storage & file I/O
│   ├── parser/
│   │   └── html.roast      # HTML & JSON parsing
│   └── utils/
│       └── helpers.roast   # Logging, timer, URL utilities
├── tests/
│   └── test_scraper.roast
└── cooked/                 # Build output
    └── release/
        └── async_scraper   # Native binary
```

## Modules

### `http/client.roast`
- `HttpClient` - Async HTTP client with timeout and retries
- `Response` - HTTP response object
- `fetch_all()` - Concurrent batch fetching

### `storage/store.roast`
- `DataStore` - Thread-safe in-memory storage
- `FileStorage` - Async file operations

### `parser/html.roast`
- `HtmlParser` - Extract titles, text, and links
- `JsonParser` - Parse JSON responses

### `utils/helpers.roast`
- `Logger` - Structured logging
- `Timer` - Execution timing
- URL and string utilities

## Usage

```bash
# Navigate to project
cd examples/async_scraper

# Type check
roastc check src/main.roast

# Run (native LLVM compilation)
roastc run src/main.roast

# Build release binary
kitchen build --release

# Run the compiled binary
./cooked/release/async_scraper
```

## Example Output

```
============================================================
🔥 Roast Async Web Scraper
   Demonstrating: async/await, classes, multi-file imports
============================================================

Scraping 5 URLs...

[Scraper] INFO: Scraping: https://example.com
[HTTP] GET https://example.com
[Scraper] INFO: Scraped: Example (2 links)

[Scraper] INFO: Scraping: https://api.github.com/users/roast-lang
[HTTP] GET https://api.github.com/users/roast-lang
[Scraper] INFO: Scraped: GitHub API (0 links)

========================================
       WEB SCRAPER REPORT
========================================
Total URLs:     5
Successful:     5
Failed:         0
Total Links:    8
========================================

✓ Scraping completed successfully!
```

## Design Patterns Used

1. **Factory Pattern** - `create_client()`, `create_logger()`, etc.
2. **Builder Pattern** - ScraperConfig with defaults
3. **Async/Await** - Non-blocking I/O
4. **Module Pattern** - Separated concerns

## License

MIT
