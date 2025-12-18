# Web Fetcher

A simple Roast application that fetches data from websites and saves to files.

## Features

- 🌐 HTTP GET requests (async)
- 📁 File I/O (save & read)
- 🔄 Async/await support
- 📝 HTML and JSON processing

## Usage

```bash
# Navigate to project
cd examples/web_fetcher

# Build the project
kitchen build

# Run the application
kitchen run

# Or run directly
roastc run src/main.roast
```

## Output

The application will:
1. Fetch `https://example.com` → save to `example_page.txt`
2. Fetch `https://jsonplaceholder.typicode.com/posts` → save to `posts.json`

## Project Structure

```
web_fetcher/
├── roast.toml       # Project config
├── src/
│   └── main.roast   # Main application
└── cooked/          # Build output (after build)
    ├── debug/
    └── release/
```

## Example Output

```
==================================================
🔥 Web Fetcher - Roast Demo Application
==================================================

--- Fetching: https://example.com ---
Fetching: https://example.com
Saving to: example_page.txt
Saved 156 bytes to example_page.txt
✓ Successfully saved to example_page.txt
Preview:
Example Domain
This domain is for use in illustrative examples.
...

--- Fetching: https://jsonplaceholder.typicode.com/posts ---
Fetching: https://jsonplaceholder.typicode.com/posts
Saving to: posts.json
✓ Successfully saved to posts.json

==================================================
Done! Files saved to current directory.
==================================================
```

## License

MIT
