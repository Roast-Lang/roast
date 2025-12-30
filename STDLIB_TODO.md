# Roast Standard Library - Status

## ✅ Recently Implemented (Dec 30, 2025)

| Module | Description |
|--------|-------------|
| `url` | URL parsing and encoding |
| `cli` | Argument parsing, flags, help generation |
| `uuid` | UUID v4/v7 generation |
| `statistics` | mean, median, stdev, variance, correlation |
| `yaml` | YAML serialization/deserialization |
| `toml` | TOML serialization/deserialization |

---

## ❌ Remaining Modules to Implement

### Medium Priority

| Module | Description |
|--------|-------------|
| `matrix` / `linalg` | Linear algebra operations |
| `reflect` | Runtime type reflection |
| `mock` | Mocking framework for tests |

---

## ✅ Already Implemented (60 modules)

`async_io`, `async_utils`, `base64`, `bisect`, `builtins`, `channel`, `cli`, `collections`, `compression`, `coverage`, `crypto`, `csv`, `database`, `doc`, `duration`, `error`, `fmt`, `fs`, `functools`, `glob`, `graph`, `hash`, `heap`, `heapq`, `hex`, `http`, `io`, `itertools`, `json`, `logging`, `math`, `metrics`, `mmap`, `net`, `path`, `profiler`, `queue`, `random`, `regex`, `result`, `shutil`, `signal`, `statistics`, `string`, `structured`, `subprocess`, `sync`, `tempfile`, `testing`, `thread`, `time`, `timezone`, `toml`, `tracing`, `url`, `uuid`, `web`, `xml`, `yaml`

---

## 🟡 Needs Verification

- `encryption` - check if covered by `crypto` module
- `benchmarks` - check `profiler` module
- `os` - environment vars may be in `builtins`
- `binary` - byte operations may be in `io`
