# ForgeDB FFI Examples

This directory contains working examples demonstrating how to use ForgeDB from various programming languages via FFI.

## Prerequisites

Build the ForgeDB FFI library first:

```bash
cd ../../..  # Go to repository root
cargo build -p forgedb-ffi --release
```

The compiled library will be at:
- Linux: `target/release/libforgedb_ffi.so`
- macOS: `target/release/libforgedb_ffi.dylib`
- Windows: `target/release/forgedb_ffi.dll`

## Examples by Language

### C Examples

Location: `c/`

**basic_example.c** - Basic database operations
- Opening/closing database
- Getting records
- Listing records
- Query execution
- Proper memory management

**error_handling.c** - Error handling patterns
- Checking error codes
- Reading error messages
- Handling different error types
- Cleanup of error objects

**Build and run:**
```bash
cd c/
make
make run
```

Or manually:
```bash
cd c/
gcc -o basic_example basic_example.c -I../../include -L../../../target/release -lforgedb_ffi
LD_LIBRARY_PATH=../../../target/release ./basic_example
```

### Python Examples

Location: `python/`

**basic_example.py** - Low-level ctypes usage
- Direct FFI function calls
- Manual memory management
- Error handling

**forgedb_client.py** - High-level Pythonic wrapper
- Context manager support
- Automatic memory management
- Pythonic error handling
- Type hints

**Run:**
```bash
cd python/
python basic_example.py
python forgedb_client.py
```

Or use the wrapper in your code:
```python
from forgedb_client import ForgeDBClient

with ForgeDBClient("./data") as db:
    user = db.get("User", 123)
    users = db.list("User", limit=10)
```

## Creating Your Own Bindings

See the main [README.md](../README.md) for guidance on creating bindings for other languages including:
- Node.js (JavaScript)
- Bun (TypeScript)
- Go
- Ruby
- Rust

Each example demonstrates:
1. Loading the FFI library
2. Defining opaque pointer types
3. Declaring function signatures
4. Proper memory management
5. Error handling
6. Creating high-level wrappers

## Testing Examples

### Create a Test Database

You can create a simple test database for the examples:

```bash
# From repository root
cd crates/ffi/examples
mkdir -p data
```

Then use ForgeDB tools to populate it, or the examples will create it automatically when using `FORGEDB_OPEN_CREATE` flag.

### Memory Leak Testing

Test C examples for memory leaks with Valgrind (Linux):

```bash
cd c/
valgrind --leak-check=full --show-leak-kinds=all ./basic_example ./data
```

## Common Issues

### Library Not Found

**Error:**
```
error while loading shared libraries: libforgedb_ffi.so: cannot open shared object file
```

**Solution:**
Set library path:
```bash
# Linux
export LD_LIBRARY_PATH=../../../target/release:$LD_LIBRARY_PATH

# macOS
export DYLD_LIBRARY_PATH=../../../target/release:$DYLD_LIBRARY_PATH
```

### Python Import Error

**Error:**
```
OSError: cannot load library '../../../target/release/libforgedb_ffi.so'
```

**Solution:**
Ensure the library is built and path is correct:
```bash
cd ../../..  # Repository root
cargo build -p forgedb-ffi --release
cd crates/ffi/examples/python
python basic_example.py
```

## Contributing Examples

When adding examples for new languages:

1. Create a new directory for the language
2. Include at least one basic example
3. Show proper error handling
4. Demonstrate memory management
5. Add a language-specific README if needed
6. Update this README with the new language
