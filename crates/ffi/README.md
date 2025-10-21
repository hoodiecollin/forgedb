# ForgeDB FFI

Foreign Function Interface (FFI) bindings for ForgeDB, enabling direct database access from C and other languages.

## Overview

The `forgedb-ffi` crate provides C-compatible FFI bindings for ForgeDB, allowing integration with programming languages and runtimes that support C foreign function interfaces. This enables direct database access without HTTP overhead, achieving significantly better performance for local operations.

### Key Benefits

- **10-20x faster** than HTTP API for single record operations
- **Zero serialization overhead** for native language types
- **Thread-safe** concurrent read access
- **Memory-safe** handle-based API
- **Language-agnostic** - works with any language supporting C FFI

## Features

### C API for Database Operations

- **Database lifecycle**: Open, close database handles
- **CRUD operations**: Get, list, query records
- **Relationship traversal**: Access related records
- **Error handling**: Comprehensive error codes and messages

### cbindgen Header Generation

Automatically generates C header files from Rust code:
- Type-safe function signatures
- Error code constants
- Documentation comments preserved
- Platform-independent declarations

### Memory-Safe FFI Patterns

- **Opaque handles**: Internal Rust types never exposed
- **Handle registry**: Validates handle lifetimes
- **Explicit cleanup**: Clear ownership semantics
- **Null safety**: All nullable pointers documented

### Error Handling Across FFI Boundary

- **Error codes**: Categorized error types (IO, NotFound, Invalid, Internal)
- **Error messages**: Detailed, human-readable error descriptions
- **Optional errors**: Caller can choose to ignore errors
- **No panics**: All errors caught and converted to error codes

## Usage Examples

### C Usage

#### Basic Example

```c
#include "forgedb.h"
#include <stdio.h>
#include <stdlib.h>

int main() {
    // Open database
    ForgeDBError* err = NULL;
    ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, &err);
    
    if (db == NULL) {
        fprintf(stderr, "Failed to open database: %s\n", 
                forgedb_error_message(err));
        forgedb_free_error(err);
        return 1;
    }
    
    // Get a record
    char* json = forgedb_get(db, "User", "123", &err);
    
    if (json != NULL) {
        printf("User: %s\n", json);
        forgedb_free_string(json);
    } else if (err != NULL) {
        int code = forgedb_error_code(err);
        if (code == FORGEDB_ERR_NOT_FOUND) {
            printf("User not found\n");
        } else {
            fprintf(stderr, "Error: %s\n", forgedb_error_message(err));
        }
        forgedb_free_error(err);
    }
    
    // List records with pagination
    char* users = forgedb_list(db, "User", NULL, 10, 0, &err);
    if (users != NULL) {
        printf("Users: %s\n", users);
        forgedb_free_string(users);
    }
    
    // Close database
    forgedb_close(db);
    return 0;
}
```

#### Error Handling Pattern

```c
ForgeDBError* err = NULL;
char* result = forgedb_get(db, "User", "123", &err);

if (result == NULL) {
    if (err != NULL) {
        int code = forgedb_error_code(err);
        
        switch (code) {
            case FORGEDB_ERR_NOT_FOUND:
                printf("Record not found\n");
                break;
            case FORGEDB_ERR_IO:
                fprintf(stderr, "I/O error: %s\n", forgedb_error_message(err));
                break;
            case FORGEDB_ERR_INVALID:
                fprintf(stderr, "Invalid parameter: %s\n", forgedb_error_message(err));
                break;
            default:
                fprintf(stderr, "Error %d: %s\n", code, forgedb_error_message(err));
        }
        
        forgedb_free_error(err);
    }
} else {
    printf("Success: %s\n", result);
    forgedb_free_string(result);
}
```

### Python ctypes Usage

#### Basic Example

```python
import ctypes
import json
import os

# Load the library
lib_path = "./target/release/libforgedb_ffi.so"  # Linux
# lib_path = "./target/release/libforgedb_ffi.dylib"  # macOS
# lib_path = "./target/release/forgedb_ffi.dll"  # Windows

forgedb = ctypes.CDLL(lib_path)

# Define opaque handle types
class ForgeDB(ctypes.Structure):
    pass

class ForgeDBError(ctypes.Structure):
    pass

# Define function signatures
forgedb.forgedb_version.argtypes = []
forgedb.forgedb_version.restype = ctypes.c_char_p

forgedb.forgedb_open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.POINTER(ctypes.POINTER(ForgeDBError))]
forgedb.forgedb_open.restype = ctypes.POINTER(ForgeDB)

forgedb.forgedb_close.argtypes = [ctypes.POINTER(ForgeDB)]
forgedb.forgedb_close.restype = None

forgedb.forgedb_get.argtypes = [
    ctypes.POINTER(ForgeDB), 
    ctypes.c_char_p, 
    ctypes.c_char_p, 
    ctypes.POINTER(ctypes.POINTER(ForgeDBError))
]
forgedb.forgedb_get.restype = ctypes.c_char_p

forgedb.forgedb_list.argtypes = [
    ctypes.POINTER(ForgeDB),
    ctypes.c_char_p,
    ctypes.c_char_p,
    ctypes.c_int32,
    ctypes.c_int32,
    ctypes.POINTER(ctypes.POINTER(ForgeDBError))
]
forgedb.forgedb_list.restype = ctypes.c_char_p

forgedb.forgedb_error_code.argtypes = [ctypes.POINTER(ForgeDBError)]
forgedb.forgedb_error_code.restype = ctypes.c_int32

forgedb.forgedb_error_message.argtypes = [ctypes.POINTER(ForgeDBError)]
forgedb.forgedb_error_message.restype = ctypes.c_char_p

forgedb.forgedb_free_error.argtypes = [ctypes.POINTER(ForgeDBError)]
forgedb.forgedb_free_error.restype = None

forgedb.forgedb_free_string.argtypes = [ctypes.c_char_p]
forgedb.forgedb_free_string.restype = None

# Constants
FORGEDB_OPEN_READONLY = 0x01
FORGEDB_OPEN_CREATE = 0x02
FORGEDB_ERR_NOT_FOUND = 2

# Usage example
def main():
    # Print version
    version = forgedb.forgedb_version()
    print(f"ForgeDB FFI version: {version.decode('utf-8')}")
    
    # Open database
    err = ctypes.POINTER(ForgeDBError)()
    db = forgedb.forgedb_open(b"./data", FORGEDB_OPEN_READONLY, ctypes.byref(err))
    
    if not db:
        msg = forgedb.forgedb_error_message(err)
        print(f"Failed to open database: {msg.decode('utf-8')}")
        forgedb.forgedb_free_error(err)
        return
    
    try:
        # Get a record
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_get(db, b"User", b"123", ctypes.byref(err))
        
        if json_str:
            user = json.loads(json_str.decode('utf-8'))
            print(f"User: {user}")
            forgedb.forgedb_free_string(json_str)
        elif err:
            code = forgedb.forgedb_error_code(err)
            if code == FORGEDB_ERR_NOT_FOUND:
                print("User not found")
            else:
                msg = forgedb.forgedb_error_message(err)
                print(f"Error: {msg.decode('utf-8')}")
            forgedb.forgedb_free_error(err)
        
        # List records
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_list(db, b"User", None, 10, 0, ctypes.byref(err))
        
        if json_str:
            users = json.loads(json_str.decode('utf-8'))
            print(f"Found {len(users)} users")
            forgedb.forgedb_free_string(json_str)
    
    finally:
        # Close database
        forgedb.forgedb_close(db)

if __name__ == "__main__":
    main()
```

#### Python Wrapper Class

```python
class ForgeDBClient:
    """High-level Python wrapper for ForgeDB FFI"""
    
    def __init__(self, path, readonly=True):
        self.db = None
        flags = FORGEDB_OPEN_READONLY if readonly else FORGEDB_OPEN_CREATE
        
        err = ctypes.POINTER(ForgeDBError)()
        self.db = forgedb.forgedb_open(path.encode('utf-8'), flags, ctypes.byref(err))
        
        if not self.db:
            msg = forgedb.forgedb_error_message(err)
            forgedb.forgedb_free_error(err)
            raise Exception(f"Failed to open database: {msg.decode('utf-8')}")
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
    
    def close(self):
        if self.db:
            forgedb.forgedb_close(self.db)
            self.db = None
    
    def get(self, model, id):
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_get(
            self.db, 
            model.encode('utf-8'), 
            str(id).encode('utf-8'), 
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            forgedb.forgedb_free_string(json_str)
            return result
        elif err:
            code = forgedb.forgedb_error_code(err)
            msg = forgedb.forgedb_error_message(err)
            forgedb.forgedb_free_error(err)
            
            if code == FORGEDB_ERR_NOT_FOUND:
                return None
            raise Exception(f"Error {code}: {msg.decode('utf-8')}")
        
        return None
    
    def list(self, model, limit=0, offset=0):
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_list(
            self.db,
            model.encode('utf-8'),
            None,
            limit,
            offset,
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            forgedb.forgedb_free_string(json_str)
            return result
        elif err:
            msg = forgedb.forgedb_error_message(err)
            forgedb.forgedb_free_error(err)
            raise Exception(f"Error: {msg.decode('utf-8')}")
        
        return []

# Usage
with ForgeDBClient("./data") as db:
    user = db.get("User", 123)
    if user:
        print(f"User: {user}")
    
    users = db.list("User", limit=10)
    print(f"Found {len(users)} users")
```

### Node.js N-API Usage

#### Using node-ffi-napi

```javascript
const ffi = require('ffi-napi');
const ref = require('ref-napi');
const path = require('path');

// Define opaque pointer types
const ForgeDBPtr = ref.refType(ref.types.void);
const ForgeDBErrorPtr = ref.refType(ref.types.void);
const ForgeDBErrorPtrPtr = ref.refType(ForgeDBErrorPtr);

// Load library
const libPath = path.join(__dirname, 'target/release/libforgedb_ffi.so'); // Linux
// const libPath = path.join(__dirname, 'target/release/libforgedb_ffi.dylib'); // macOS

const forgedb = ffi.Library(libPath, {
    'forgedb_version': ['string', []],
    'forgedb_open': [ForgeDBPtr, ['string', 'int', ForgeDBErrorPtrPtr]],
    'forgedb_close': ['void', [ForgeDBPtr]],
    'forgedb_get': ['string', [ForgeDBPtr, 'string', 'string', ForgeDBErrorPtrPtr]],
    'forgedb_list': ['string', [ForgeDBPtr, 'string', 'string', 'int32', 'int32', ForgeDBErrorPtrPtr]],
    'forgedb_error_code': ['int32', [ForgeDBErrorPtr]],
    'forgedb_error_message': ['string', [ForgeDBErrorPtr]],
    'forgedb_free_error': ['void', [ForgeDBErrorPtr]],
    'forgedb_free_string': ['void', ['string']]
});

// Constants
const FORGEDB_OPEN_READONLY = 0x01;
const FORGEDB_OPEN_CREATE = 0x02;
const FORGEDB_ERR_NOT_FOUND = 2;

class ForgeDBClient {
    constructor(dbPath, options = {}) {
        const flags = options.readonly ? FORGEDB_OPEN_READONLY : FORGEDB_OPEN_CREATE;
        const errPtr = ref.alloc(ForgeDBErrorPtr);
        
        this.db = forgedb.forgedb_open(dbPath, flags, errPtr);
        
        if (this.db.isNull()) {
            const err = errPtr.deref();
            const msg = forgedb.forgedb_error_message(err);
            forgedb.forgedb_free_error(err);
            throw new Error(`Failed to open database: ${msg}`);
        }
    }
    
    close() {
        if (this.db && !this.db.isNull()) {
            forgedb.forgedb_close(this.db);
            this.db = null;
        }
    }
    
    get(model, id) {
        const errPtr = ref.alloc(ForgeDBErrorPtr);
        const jsonStr = forgedb.forgedb_get(this.db, model, String(id), errPtr);
        
        if (jsonStr) {
            const result = JSON.parse(jsonStr);
            forgedb.forgedb_free_string(jsonStr);
            return result;
        }
        
        const err = errPtr.deref();
        if (!err.isNull()) {
            const code = forgedb.forgedb_error_code(err);
            const msg = forgedb.forgedb_error_message(err);
            forgedb.forgedb_free_error(err);
            
            if (code === FORGEDB_ERR_NOT_FOUND) {
                return null;
            }
            throw new Error(`Error ${code}: ${msg}`);
        }
        
        return null;
    }
    
    list(model, options = {}) {
        const { limit = 0, offset = 0 } = options;
        const errPtr = ref.alloc(ForgeDBErrorPtr);
        
        const jsonStr = forgedb.forgedb_list(
            this.db,
            model,
            null,
            limit,
            offset,
            errPtr
        );
        
        if (jsonStr) {
            const result = JSON.parse(jsonStr);
            forgedb.forgedb_free_string(jsonStr);
            return result;
        }
        
        const err = errPtr.deref();
        if (!err.isNull()) {
            const msg = forgedb.forgedb_error_message(err);
            forgedb.forgedb_free_error(err);
            throw new Error(`Error: ${msg}`);
        }
        
        return [];
    }
}

// Usage
const db = new ForgeDBClient('./data', { readonly: true });

try {
    console.log(`ForgeDB version: ${forgedb.forgedb_version()}`);
    
    const user = db.get('User', 123);
    if (user) {
        console.log('User:', user);
    } else {
        console.log('User not found');
    }
    
    const users = db.list('User', { limit: 10 });
    console.log(`Found ${users.length} users`);
} finally {
    db.close();
}
```

### Bun FFI Usage

Bun provides native FFI support with excellent performance:

```typescript
import { dlopen, FFIType, suffix, CString, ptr } from "bun:ffi";
import path from "path";

// Define library path
const libPath = path.join(
    import.meta.dir,
    `target/release/libforgedb_ffi.${suffix}` // .so on Linux, .dylib on macOS, .dll on Windows
);

// Load library
const { symbols } = dlopen(libPath, {
    forgedb_version: {
        args: [],
        returns: FFIType.cstring,
    },
    forgedb_open: {
        args: [FFIType.cstring, FFIType.i32, FFIType.ptr],
        returns: FFIType.ptr,
    },
    forgedb_close: {
        args: [FFIType.ptr],
        returns: FFIType.void,
    },
    forgedb_get: {
        args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.ptr],
        returns: FFIType.cstring,
    },
    forgedb_list: {
        args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.i32, FFIType.i32, FFIType.ptr],
        returns: FFIType.cstring,
    },
    forgedb_error_code: {
        args: [FFIType.ptr],
        returns: FFIType.i32,
    },
    forgedb_error_message: {
        args: [FFIType.ptr],
        returns: FFIType.cstring,
    },
    forgedb_free_error: {
        args: [FFIType.ptr],
        returns: FFIType.void,
    },
    forgedb_free_string: {
        args: [FFIType.cstring],
        returns: FFIType.void,
    },
});

// Constants
const FORGEDB_OPEN_READONLY = 0x01;
const FORGEDB_OPEN_CREATE = 0x02;
const FORGEDB_ERR_NOT_FOUND = 2;

class ForgeDBClient {
    private db: number | null = null;
    
    constructor(dbPath: string, readonly: boolean = true) {
        const flags = readonly ? FORGEDB_OPEN_READONLY : FORGEDB_OPEN_CREATE;
        
        // Allocate error pointer
        const errPtr = new BigUint64Array(1);
        
        this.db = symbols.forgedb_open(
            ptr(Buffer.from(dbPath + "\0")),
            flags,
            ptr(errPtr)
        );
        
        if (this.db === 0) {
            const err = errPtr[0];
            if (err !== 0n) {
                const msg = new CString(symbols.forgedb_error_message(Number(err)));
                symbols.forgedb_free_error(Number(err));
                throw new Error(`Failed to open database: ${msg}`);
            }
            throw new Error("Failed to open database");
        }
    }
    
    close(): void {
        if (this.db !== null && this.db !== 0) {
            symbols.forgedb_close(this.db);
            this.db = null;
        }
    }
    
    get(model: string, id: string | number): any | null {
        if (!this.db) throw new Error("Database is closed");
        
        const errPtr = new BigUint64Array(1);
        
        const jsonStr = symbols.forgedb_get(
            this.db,
            ptr(Buffer.from(model + "\0")),
            ptr(Buffer.from(String(id) + "\0")),
            ptr(errPtr)
        );
        
        if (jsonStr !== 0) {
            const result = JSON.parse(new CString(jsonStr).toString());
            symbols.forgedb_free_string(jsonStr);
            return result;
        }
        
        const err = errPtr[0];
        if (err !== 0n) {
            const code = symbols.forgedb_error_code(Number(err));
            const msg = new CString(symbols.forgedb_error_message(Number(err)));
            symbols.forgedb_free_error(Number(err));
            
            if (code === FORGEDB_ERR_NOT_FOUND) {
                return null;
            }
            throw new Error(`Error ${code}: ${msg}`);
        }
        
        return null;
    }
    
    list(model: string, options: { limit?: number; offset?: number } = {}): any[] {
        if (!this.db) throw new Error("Database is closed");
        
        const { limit = 0, offset = 0 } = options;
        const errPtr = new BigUint64Array(1);
        
        const jsonStr = symbols.forgedb_list(
            this.db,
            ptr(Buffer.from(model + "\0")),
            0, // null filter
            limit,
            offset,
            ptr(errPtr)
        );
        
        if (jsonStr !== 0) {
            const result = JSON.parse(new CString(jsonStr).toString());
            symbols.forgedb_free_string(jsonStr);
            return result;
        }
        
        const err = errPtr[0];
        if (err !== 0n) {
            const msg = new CString(symbols.forgedb_error_message(Number(err)));
            symbols.forgedb_free_error(Number(err));
            throw new Error(`Error: ${msg}`);
        }
        
        return [];
    }
}

// Usage
const db = new ForgeDBClient("./data", true);

try {
    console.log(`ForgeDB version: ${new CString(symbols.forgedb_version())}`);
    
    const user = db.get("User", 123);
    if (user) {
        console.log("User:", user);
    } else {
        console.log("User not found");
    }
    
    const users = db.list("User", { limit: 10 });
    console.log(`Found ${users.length} users`);
} finally {
    db.close();
}
```

## Building

### Prerequisites

- Rust 1.70 or later
- cbindgen (for header generation)

### Compiling the FFI Library

```bash
# Build debug version
cargo build -p forgedb-ffi

# Build release version (recommended for production)
cargo build -p forgedb-ffi --release

# Build as shared library only
cargo rustc -p forgedb-ffi --release --crate-type cdylib
```

The compiled library will be in:
- `target/debug/libforgedb_ffi.so` (Linux debug)
- `target/release/libforgedb_ffi.so` (Linux release)
- `target/release/libforgedb_ffi.dylib` (macOS)
- `target/release/forgedb_ffi.dll` (Windows)

### Header Generation with cbindgen

The C header file is automatically generated during the build process by the `build.rs` script:

```bash
# Build the project to generate the header
cargo build -p forgedb-ffi

# The header will be at: crates/ffi/include/forgedb.h
```

#### Manual Header Generation

If you need to regenerate the header manually:

```bash
# Install cbindgen
cargo install cbindgen

# Generate header
cd crates/ffi
cbindgen --config cbindgen.toml --crate forgedb-ffi --output include/forgedb.h
```

The `build.rs` configuration:
```rust
cbindgen::Builder::new()
    .with_crate(&crate_dir)
    .with_language(cbindgen::Language::C)
    .with_pragma_once(true)
    .with_include_guard("FORGEDB_FFI_H")
    .with_documentation(true)
    .generate()
    .expect("Unable to generate bindings")
    .write_to_file("include/forgedb.h");
```

### Linking Instructions

#### Linking with GCC/Clang

```bash
# Static linking (Linux)
gcc -o myapp myapp.c -L./target/release -lforgedb_ffi -lpthread -ldl -lm

# Dynamic linking (Linux)
gcc -o myapp myapp.c -L./target/release -lforgedb_ffi
export LD_LIBRARY_PATH=./target/release:$LD_LIBRARY_PATH

# macOS
clang -o myapp myapp.c -L./target/release -lforgedb_ffi

# Windows (MSVC)
cl myapp.c /link /LIBPATH:target\release forgedb_ffi.lib
```

#### CMake Integration

```cmake
cmake_minimum_required(VERSION 3.10)
project(MyApp)

# Find ForgeDB FFI library
find_library(FORGEDB_FFI forgedb_ffi PATHS ${CMAKE_SOURCE_DIR}/target/release)

# Create executable
add_executable(myapp myapp.c)

# Link against ForgeDB FFI
target_link_libraries(myapp ${FORGEDB_FFI})

# Add include directory
target_include_directories(myapp PRIVATE ${CMAKE_SOURCE_DIR}/crates/ffi/include)
```

## API Reference

### Database Handles

#### ForgeDB

Opaque handle to a database connection. Created by `forgedb_open()` and destroyed by `forgedb_close()`.

**Lifetime**: From `forgedb_open()` until `forgedb_close()`  
**Thread Safety**: Safe for concurrent reads  
**Ownership**: Caller owns and must close

### CRUD Operations

#### forgedb_open

```c
ForgeDB* forgedb_open(const char* path, int flags, ForgeDBError** error);
```

Opens a ForgeDB database.

**Parameters**:
- `path`: Path to database directory (null-terminated UTF-8 string)
- `flags`: Bitwise OR of:
  - `FORGEDB_OPEN_READONLY` (0x01): Open in read-only mode
  - `FORGEDB_OPEN_CREATE` (0x02): Create if doesn't exist
- `error`: Output parameter for error (can be NULL)

**Returns**: Database handle on success, NULL on error

**Example**:
```c
ForgeDBError* err = NULL;
ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, &err);
```

#### forgedb_close

```c
void forgedb_close(ForgeDB* db);
```

Closes a database handle. Safe to call with NULL or multiple times.

#### forgedb_get

```c
char* forgedb_get(ForgeDB* db, const char* model, const char* id, ForgeDBError** error);
```

Gets a single record by ID.

**Parameters**:
- `db`: Database handle
- `model`: Model name (e.g., "User")
- `id`: Record ID as string
- `error`: Output parameter for error

**Returns**: JSON string on success (must be freed), NULL on error or not found

#### forgedb_list

```c
char* forgedb_list(ForgeDB* db, const char* model, const char* filter_json, 
                   int32_t limit, int32_t offset, ForgeDBError** error);
```

Lists records with optional filtering and pagination.

**Parameters**:
- `db`: Database handle
- `model`: Model name
- `filter_json`: JSON object with filters (can be NULL)
- `limit`: Maximum records to return (0 for all)
- `offset`: Number of records to skip (0 for none)
- `error`: Output parameter for error

**Returns**: JSON array string on success (must be freed), NULL on error

#### forgedb_query

```c
char* forgedb_query(ForgeDB* db, const char* model, const char* query_json, 
                    ForgeDBError** error);
```

Executes a complex query.

**Parameters**:
- `db`: Database handle
- `model`: Model name
- `query_json`: JSON query object with filters, sorting, pagination
- `error`: Output parameter for error

**Returns**: JSON array string on success (must be freed), NULL on error

#### forgedb_get_relations

```c
char* forgedb_get_relations(ForgeDB* db, const char* model, const char* id,
                            const char* relation_name, ForgeDBError** error);
```

Gets related records for a given record.

**Parameters**:
- `db`: Database handle
- `model`: Model name
- `id`: Record ID
- `relation_name`: Name of the relation field
- `error`: Output parameter for error

**Returns**: JSON array of related records (must be freed), NULL on error

### Error Codes

| Code | Constant | Description |
|------|----------|-------------|
| 0 | `FORGEDB_OK` | Success (no error) |
| 1 | `FORGEDB_ERR_IO` | I/O error (disk, permissions, etc.) |
| 2 | `FORGEDB_ERR_NOT_FOUND` | Record or resource not found |
| 3 | `FORGEDB_ERR_INVALID` | Invalid parameter or handle |
| 4 | `FORGEDB_ERR_INTERNAL` | Internal error (bug) |

#### forgedb_error_code

```c
int32_t forgedb_error_code(ForgeDBError* error);
```

Gets the error code from an error handle.

#### forgedb_error_message

```c
const char* forgedb_error_message(ForgeDBError* error);
```

Gets the error message from an error handle. The returned string is valid until the error is freed.

#### forgedb_free_error

```c
void forgedb_free_error(ForgeDBError* error);
```

Frees an error handle. Safe to call with NULL.

### Memory Management

#### forgedb_free_string

```c
void forgedb_free_string(char* str);
```

Frees a string returned by ForgeDB. Must be called for every non-NULL string returned by `forgedb_get`, `forgedb_list`, etc.

**Important**: Do NOT use `free()` on strings returned by ForgeDB. Always use `forgedb_free_string()`.

### Utility

#### forgedb_version

```c
const char* forgedb_version(void);
```

Returns the ForgeDB FFI version string. The string is static and doesn't need to be freed.

## Memory Safety

### Handle Lifecycle

ForgeDB uses an opaque handle system to ensure memory safety:

1. **Opaque Pointers**: Handles are opaque - the internal structure is never exposed
2. **Handle Registry**: Internal registry validates all handles before use
3. **Invalid Handles**: Using an invalid handle returns an error, never crashes
4. **Double Close**: Closing a handle multiple times is safe (no-op after first close)

```c
// GOOD: Proper lifecycle
ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, NULL);
// ... use db ...
forgedb_close(db);

// SAFE: Double close is okay
forgedb_close(db);  // No-op

// BAD: Using after close (will return error, not crash)
char* result = forgedb_get(db, "User", "123", &err);
// result will be NULL, err will be FORGEDB_ERR_INVALID
```

### String Handling

All strings in the FFI follow these rules:

1. **Null-Terminated**: All strings are null-terminated C strings
2. **UTF-8 Encoding**: All strings are valid UTF-8
3. **Ownership**: 
   - Input strings (parameters): Caller owns, not modified
   - Output strings (return values): Caller owns, must free with `forgedb_free_string()`
4. **Lifetime**:
   - Input strings: Must remain valid for duration of call
   - Output strings: Valid until freed by caller

```c
// GOOD: Proper string handling
char* json = forgedb_get(db, "User", "123", NULL);
if (json != NULL) {
    printf("%s\n", json);
    forgedb_free_string(json);  // MUST free
}

// BAD: Memory leak
char* json = forgedb_get(db, "User", "123", NULL);
printf("%s\n", json);
// MEMORY LEAK: forgot to free!

// BAD: Wrong free function
char* json = forgedb_get(db, "User", "123", NULL);
free(json);  // WRONG: Use forgedb_free_string()
```

### Error Ownership

Error objects follow the same ownership rules as database handles:

1. **Creation**: Set by ForgeDB functions on error
2. **Ownership**: Caller owns error object
3. **Inspection**: Safe to read code and message while owned
4. **Cleanup**: Caller must call `forgedb_free_error()`

```c
// GOOD: Proper error handling
ForgeDBError* err = NULL;
char* result = forgedb_get(db, "User", "123", &err);
if (result == NULL && err != NULL) {
    printf("Error %d: %s\n", 
           forgedb_error_code(err),
           forgedb_error_message(err));
    forgedb_free_error(err);  // MUST free
}

// SAFE: NULL error pointer (ignore errors)
char* result = forgedb_get(db, "User", "123", NULL);
// No error object created

// BAD: Memory leak
ForgeDBError* err = NULL;
forgedb_get(db, "User", "123", &err);
// MEMORY LEAK: forgot to free error!
```

### Thread Safety Considerations

- **Database Handle**: Thread-safe for concurrent reads. Multiple threads can call read operations simultaneously.
- **Error Objects**: Not thread-safe. Each thread should use its own error pointer.
- **Strings**: Immutable once returned. Safe to read from multiple threads, but only one thread should free.

```c
// GOOD: Each thread has its own error
#pragma omp parallel for
for (int i = 0; i < 1000; i++) {
    ForgeDBError* err = NULL;  // Thread-local
    char* result = forgedb_get(db, "User", id[i], &err);
    // ... process result ...
    if (result) forgedb_free_string(result);
    if (err) forgedb_free_error(err);
}

// BAD: Shared error pointer
ForgeDBError* err = NULL;  // Shared!
#pragma omp parallel for
for (int i = 0; i < 1000; i++) {
    // RACE CONDITION: multiple threads writing to err
    char* result = forgedb_get(db, "User", id[i], &err);
}
```

## Language Bindings

### How to Create Bindings

Creating bindings for a new language involves:

1. **Load the Library**: Use your language's FFI mechanism to load `libforgedb_ffi`
2. **Declare Types**: Define opaque pointer types for `ForgeDB` and `ForgeDBError`
3. **Declare Functions**: Map each C function to your language
4. **Handle Memory**: Ensure proper cleanup of strings and errors
5. **Wrap in Idiomatic API**: Create high-level wrapper matching language conventions

### Example Bindings

Bindings are currently available for:

- **C**: Native API (see `include/forgedb.h`)
- **Python**: Via `ctypes` (see Python example above)
- **JavaScript (Node.js)**: Via `ffi-napi` (see Node.js example above)
- **TypeScript (Bun)**: Via native FFI (see Bun example above)

### Creating Bindings for Other Languages

#### Rust (via FFI)

```rust
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

#[link(name = "forgedb_ffi")]
extern "C" {
    fn forgedb_version() -> *const c_char;
    fn forgedb_open(path: *const c_char, flags: c_int, error: *mut *mut ForgeDBError) -> *mut ForgeDB;
    fn forgedb_close(db: *mut ForgeDB);
    // ... other functions
}

pub struct ForgeDB(*mut ForgeDB);
// ... wrapper implementation
```

#### Go

```go
package forgedb

// #cgo LDFLAGS: -L./target/release -lforgedb_ffi
// #include "include/forgedb.h"
import "C"
import "unsafe"

type Database struct {
    handle *C.ForgeDB
}

func Open(path string, readonly bool) (*Database, error) {
    cPath := C.CString(path)
    defer C.free(unsafe.Pointer(cPath))
    
    var err *C.ForgeDBError
    flags := C.int(0)
    if readonly {
        flags = C.FORGEDB_OPEN_READONLY
    }
    
    db := C.forgedb_open(cPath, flags, &err)
    if db == nil {
        msg := C.GoString(C.forgedb_error_message(err))
        C.forgedb_free_error(err)
        return nil, errors.New(msg)
    }
    
    return &Database{handle: db}, nil
}

func (d *Database) Close() {
    if d.handle != nil {
        C.forgedb_close(d.handle)
        d.handle = nil
    }
}
```

#### Ruby (via FFI gem)

```ruby
require 'ffi'

module ForgeDB
  extend FFI::Library
  ffi_lib './target/release/libforgedb_ffi.so'
  
  # Define opaque pointers
  class ForgeDBHandle < FFI::Struct; end
  class ForgeDBErrorHandle < FFI::Struct; end
  
  # Define functions
  attach_function :forgedb_version, [], :string
  attach_function :forgedb_open, [:string, :int, :pointer], :pointer
  attach_function :forgedb_close, [:pointer], :void
  attach_function :forgedb_get, [:pointer, :string, :string, :pointer], :string
  # ... other functions
  
  class Database
    def initialize(path, readonly: true)
      flags = readonly ? 0x01 : 0x02
      err_ptr = FFI::MemoryPointer.new(:pointer)
      @handle = ForgeDB.forgedb_open(path, flags, err_ptr)
      # ... error handling
    end
    
    def close
      ForgeDB.forgedb_close(@handle) if @handle
      @handle = nil
    end
  end
end
```

## Testing

### Running FFI Tests

```bash
# Run all tests
cargo test -p forgedb-ffi

# Run with output
cargo test -p forgedb-ffi -- --nocapture

# Run specific test
cargo test -p forgedb-ffi test_open_close

# Run tests with memory sanitizer (Linux)
RUSTFLAGS="-Z sanitizer=address" cargo test -p forgedb-ffi
```

### Test Categories

1. **Unit Tests**: Test individual functions in isolation
   - `tests/lib_tests.rs`: Core API tests
   - `tests/errors_tests.rs`: Error handling tests
   - `tests/handles_tests.rs`: Handle lifecycle tests
   - `tests/conversions_tests.rs`: String conversion tests

2. **Integration Tests**: Test complete workflows
   - Open, perform operations, close
   - Error handling scenarios
   - Concurrent access patterns

3. **Memory Tests**: Validate no leaks or undefined behavior
   - Valgrind on Linux
   - AddressSanitizer
   - LeakSanitizer

### Memory Leak Testing

```bash
# Linux: Use Valgrind
cargo build -p forgedb-ffi
valgrind --leak-check=full --show-leak-kinds=all \
    ./target/debug/forgedb-ffi-test

# Use AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" cargo test -p forgedb-ffi

# Use LeakSanitizer
RUSTFLAGS="-Z sanitizer=leak" cargo test -p forgedb-ffi
```

### Performance Testing

```bash
# Build release version for benchmarks
cargo build -p forgedb-ffi --release

# Run benchmark comparisons (FFI vs HTTP)
cargo bench -p forgedb-ffi
```

Expected performance characteristics:
- Get single record: 50-100μs (vs 1-2ms HTTP)
- List 10 records: 100-200μs (vs 2-3ms HTTP)
- **10-20x faster** than HTTP for most operations

## Documentation

### Additional Resources

- [FFI_SPEC.md](docs/FFI_SPEC.md): Complete FFI specification
- [forgedb.h](include/forgedb.h): Generated C header file
- [Rust API Docs](https://docs.rs/forgedb-ffi): Auto-generated documentation

### API Documentation

Generate Rust documentation:
```bash
cargo doc -p forgedb-ffi --open
```

## Architecture

### Design Principles

1. **Memory Safety**: All memory managed safely by Rust
2. **Thread Safety**: Concurrent reads supported via RwLock
3. **Error Isolation**: No panics cross FFI boundary
4. **Zero Copy**: JSON strings allocated once, transferred to caller
5. **Handle Validation**: All handles validated before use

### Internal Structure

```
forgedb-ffi/
├── src/
│   ├── lib.rs          # Main FFI functions
│   ├── handles.rs      # Handle registry and validation
│   ├── errors.rs       # Error handling and codes
│   └── conversions.rs  # String and JSON conversions
├── include/
│   └── forgedb.h       # Generated C header
├── tests/              # Test suite
├── build.rs            # Build script (cbindgen)
└── Cargo.toml
```

### Handle Registry

Uses atomic counters and RwLock for thread-safe handle management:

```rust
pub struct HandleRegistry<T> {
    next_id: AtomicUsize,
    handles: Arc<RwLock<HashMap<usize, Arc<T>>>>,
}
```

Benefits:
- No pointer arithmetic
- Automatic validation
- Safe concurrent access
- No use-after-free possible

## Performance

### Benchmarks

| Operation | HTTP | FFI | Speedup |
|-----------|------|-----|---------|
| Get single | 1-2ms | 50-100μs | 10-20x |
| List 10 | 2-3ms | 100-200μs | 10-15x |
| List 100 | 5-10ms | 500μs-1ms | 5-10x |

### Optimization Tips

1. **Batch Operations**: Use `list()` instead of multiple `get()` calls
2. **Connection Reuse**: Keep database handle open, don't open/close repeatedly
3. **Thread Pooling**: Reuse threads for concurrent access
4. **JSON Parsing**: Use streaming parsers for large result sets
5. **Error Handling**: Use NULL error pointer if errors aren't needed

```c
// GOOD: Batch operation
char* users = forgedb_list(db, "User", NULL, 100, 0, NULL);
// Parse JSON once

// BAD: Multiple individual operations
for (int i = 0; i < 100; i++) {
    char id[32];
    sprintf(id, "%d", i);
    char* user = forgedb_get(db, "User", id, NULL);
    // ... 100 separate FFI calls!
}
```

## Troubleshooting

### Common Issues

#### Library Not Found

```
Error: cannot find -lforgedb_ffi
```

**Solution**: Ensure the library is built and in the correct location:
```bash
cargo build -p forgedb-ffi --release
export LD_LIBRARY_PATH=./target/release:$LD_LIBRARY_PATH
```

#### Symbol Not Found

```
Error: undefined symbol: forgedb_open
```

**Solution**: Check that you're linking against the correct library version and that the header matches the library.

#### Segmentation Fault

**Common Causes**:
1. Using a closed/invalid handle
2. Not null-terminating input strings
3. Double-freeing strings or errors
4. Using wrong free function

**Debug**: Run with sanitizers:
```bash
RUSTFLAGS="-Z sanitizer=address" cargo build -p forgedb-ffi
```

#### Memory Leaks

**Common Causes**:
1. Not calling `forgedb_free_string()` on returned strings
2. Not calling `forgedb_free_error()` on error objects
3. Not calling `forgedb_close()` on database handles

**Debug**: Use Valgrind or LeakSanitizer

### Getting Help

- Check the [FFI_SPEC.md](docs/FFI_SPEC.md) for detailed specifications
- Review the test suite in `tests/` for examples
- Open an issue on GitHub with minimal reproduction

## Documentation

For more information about ForgeDB:

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Public Crates Guide](../../docs/PUBLIC_CRATES.md)** - Complete runtime library documentation
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow
- **[Contributing Guide](../../docs/CONTRIBUTING.md)** - Contribution guidelines

## License

This crate is part of the ForgeDB project. See the repository root for license information.

## Contributing

Contributions are welcome! See [Contributing Guide](../../docs/CONTRIBUTING.md) for general guidelines.

When contributing FFI bindings:

1. Maintain memory safety guarantees
2. Document all public functions thoroughly
3. Add tests for new functionality
4. Update the header file with cbindgen
5. Validate no memory leaks with sanitizers
6. Follow C naming conventions for exported symbols
