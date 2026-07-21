# ForgeDB FFI Specification

Version: 0.1.0
Status: Draft
Created: 2025-10-15

## Overview

This document specifies the C-compatible Foreign Function Interface (FFI) for ForgeDB, enabling direct database access from Bun and other runtimes without HTTP overhead.

## Design Principles

1. **Read-Only Access**: Only read operations exposed via FFI for safety
2. **Handle-Based Memory Management**: Opaque pointers with internal registry
3. **JSON String Returns**: Simple serialization, clear ownership
4. **Error Pointers**: Standard C FFI error handling pattern
5. **Thread-Safe**: Concurrent read access supported

## Memory Ownership Rules

### Database Handle

- **Creation**: `forgedb_open()` returns handle, caller owns
- **Usage**: Handle must be passed to all operations
- **Cleanup**: Caller must call `forgedb_close()` exactly once
- **Thread Safety**: Handle is thread-safe, concurrent reads allowed

**Example**:
```c
ForgeDBError* err = NULL;
ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, &err);
if (db == NULL) {
    fprintf(stderr, "Error: %s\n", forgedb_error_message(err));
    forgedb_free_error(err);
    return 1;
}

// Use database...

forgedb_close(db);
```

### Returned Strings

- **Ownership**: Caller owns all returned strings
- **Cleanup**: Caller must call `forgedb_free_string()` for each non-NULL return
- **Encoding**: All strings are UTF-8
- **Null-terminated**: All strings are null-terminated C strings
- **Lifetime**: Valid until freed by caller

**Example**:
```c
char* json = forgedb_get(db, "User", "123", &err);
if (json != NULL) {
    printf("Result: %s\n", json);
    forgedb_free_string(json);  // Must free!
}
```

### Error Objects

- **Creation**: Functions set error out-parameter on failure
- **Ownership**: Caller owns error object
- **Cleanup**: Caller must call `forgedb_free_error()`
- **Inspection**: Safe to read while owned
- **Lifetime**: Valid until freed

**Example**:
```c
ForgeDBError* err = NULL;
char* result = forgedb_get(db, "User", "123", &err);

if (result == NULL) {
    if (err != NULL) {
        fprintf(stderr, "Error %d: %s\n",
            forgedb_error_code(err),
            forgedb_error_message(err));
        forgedb_free_error(err);
    }
    return;
}

// Use result
printf("Result: %s\n", result);
forgedb_free_string(result);
```

## Thread Safety

### Guarantees

- **Database Handle**: Thread-safe for concurrent reads
- **Read Operations**: Multiple threads can call simultaneously
- **Internal Locking**: Uses `Arc<RwLock<>>` for safe concurrent access
- **Read Locks**: Multiple readers allowed, no writer blocking

### Restrictions

- **Error Objects**: Not thread-safe (one per thread recommended)
- **Strings**: Immutable once returned, safe to read from multiple threads
- **Handle Validity**: Once closed, handle invalid across all threads

### Best Practices

```c
// GOOD: Concurrent reads
ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, NULL);

#pragma omp parallel for
for (int i = 0; i < 1000; i++) {
    ForgeDBError* err = NULL;  // Per-thread error
    char id[32];
    sprintf(id, "user-%d", i);
    char* result = forgedb_get(db, "User", id, &err);
    if (result != NULL) {
        // Process result...
        forgedb_free_string(result);
    }
}

forgedb_close(db);

// BAD: Sharing error objects across threads
ForgeDBError* shared_err = NULL;  // Don't do this!
```

## Error Handling Pattern

### Error Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | `FORGEDB_OK` | Success (no error) |
| 1 | `FORGEDB_ERR_IO` | I/O error (disk, permissions, etc.) |
| 2 | `FORGEDB_ERR_NOT_FOUND` | Record or model not found |
| 3 | `FORGEDB_ERR_INVALID` | Invalid parameter or state |
| 4 | `FORGEDB_ERR_INTERNAL` | Internal error (bug) |

### Standard Error Handling

```c
ForgeDBError* err = NULL;
char* result = forgedb_get(db, "User", "123", &err);

if (result == NULL) {
    if (err != NULL) {
        int code = forgedb_error_code(err);

        switch (code) {
            case FORGEDB_ERR_NOT_FOUND:
                printf("User not found\n");
                break;
            case FORGEDB_ERR_IO:
                fprintf(stderr, "I/O error: %s\n", forgedb_error_message(err));
                break;
            default:
                fprintf(stderr, "Error %d: %s\n", code, forgedb_error_message(err));
        }

        forgedb_free_error(err);
    }
    return;
}

// Success
printf("User: %s\n", result);
forgedb_free_string(result);
```

### Error vs. Not Found

**Important**: `FORGEDB_ERR_NOT_FOUND` is not always an error condition. For `get` operations, a missing record may be expected behavior.

```c
// Check if user exists
ForgeDBError* err = NULL;
char* user = forgedb_get(db, "User", "123", &err);

if (user == NULL) {
    if (err != NULL) {
        int code = forgedb_error_code(err);
        if (code == FORGEDB_ERR_NOT_FOUND) {
            // Expected: user doesn't exist
            printf("User not found\n");
        } else {
            // Unexpected error
            fprintf(stderr, "Error: %s\n", forgedb_error_message(err));
        }
        forgedb_free_error(err);
    }
} else {
    // User exists
    printf("User: %s\n", user);
    forgedb_free_string(user);
}
```

## Return Value Conventions

### Success/Failure Indication

- **NULL**: Indicates error or not found (check error parameter)
- **Non-NULL**: Success, valid data
- **Empty String**: Valid return (e.g., empty list: `"[]"`)
- **Error Parameter**: Set to non-NULL on error, NULL on success

### Examples

```c
// Empty list (success)
char* users = forgedb_list(db, "User", NULL, 0, 0, NULL);
// users == "[]" (not NULL!)

// Not found (error)
char* user = forgedb_get(db, "User", "nonexistent", &err);
// user == NULL, err != NULL, code == FORGEDB_ERR_NOT_FOUND

// Invalid parameter (error)
char* result = forgedb_get(NULL, "User", "123", &err);
// result == NULL, err != NULL, code == FORGEDB_ERR_INVALID
```

## API Reference

### Database Lifecycle

#### `forgedb_open`

Opens a ForgeDB database.

**Signature**:
```c
ForgeDB* forgedb_open(
    const char* path,
    int flags,
    ForgeDBError** error
);
```

**Parameters**:
- `path`: Path to database directory (null-terminated UTF-8 string)
- `flags`: Bitwise OR of:
  - `FORGEDB_OPEN_READONLY` (0x01): Open in read-only mode
  - `FORGEDB_OPEN_CREATE` (0x02): Create if doesn't exist
- `error`: Output parameter for error (can be NULL)

**Returns**:
- Non-NULL handle on success
- NULL on error (check error parameter)

**Thread Safety**: Safe to call from multiple threads

**Example**:
```c
ForgeDBError* err = NULL;
ForgeDB* db = forgedb_open("./data", FORGEDB_OPEN_READONLY, &err);
if (db == NULL) {
    fprintf(stderr, "Failed to open: %s\n", forgedb_error_message(err));
    forgedb_free_error(err);
    return 1;
}
```

#### `forgedb_close`

Closes a ForgeDB database.

**Signature**:
```c
void forgedb_close(ForgeDB* db);
```

**Parameters**:
- `db`: Database handle to close

**Behavior**:
- After this call, the handle is invalid and must not be used
- Safe to call with NULL (no-op)
- Safe to call multiple times (subsequent calls are no-op)
- All resources are freed

**Thread Safety**: Not thread-safe with concurrent operations on same handle

**Example**:
```c
forgedb_close(db);
db = NULL;  // Good practice
```

### Read Operations

#### `forgedb_get`

Gets a single record by ID.

**Signature**:
```c
char* forgedb_get(
    ForgeDB* db,
    const char* model,
    const char* id,
    ForgeDBError** error
);
```

**Parameters**:
- `db`: Database handle
- `model`: Model name (e.g., "User")
- `id`: Record ID
- `error`: Output parameter for error (can be NULL)

**Returns**:
- JSON string on success (must be freed with `forgedb_free_string`)
- NULL on error or not found

**Thread Safety**: Safe to call concurrently

**Example**:
```c
char* json = forgedb_get(db, "User", "123", &err);
if (json != NULL) {
    printf("User: %s\n", json);
    forgedb_free_string(json);
}
```

#### `forgedb_list`

Lists records with optional filtering.

**Signature**:
```c
char* forgedb_list(
    ForgeDB* db,
    const char* model,
    const char* filter_json,
    int32_t limit,
    int32_t offset,
    ForgeDBError** error
);
```

**Parameters**:
- `db`: Database handle
- `model`: Model name
- `filter_json`: JSON object with filters (can be NULL)
  - Example: `{"email": "test@example.com", "verified": true}`
- `limit`: Maximum number of records (0 for all)
- `offset`: Number of records to skip (0 for none)
- `error`: Output parameter for error

**Returns**:
- JSON array string on success (must be freed)
- NULL on error

**Thread Safety**: Safe to call concurrently

**Example**:
```c
// List first 10 verified users
char* filters = "{\"verified\": true}";
char* json = forgedb_list(db, "User", filters, 10, 0, &err);
if (json != NULL) {
    printf("Users: %s\n", json);
    forgedb_free_string(json);
}
```

#### `forgedb_query`

Executes a complex query.

**Signature**:
```c
char* forgedb_query(
    ForgeDB* db,
    const char* model,
    const char* query_json,
    ForgeDBError** error
);
```

**Parameters**:
- `db`: Database handle
- `model`: Model name
- `query_json`: JSON query object
  - Example: `{"filters": {"age": {"gt": 18}}, "sort": ["name"], "limit": 10}`
- `error`: Output parameter for error

**Returns**:
- JSON array string on success (must be freed)
- NULL on error

**Thread Safety**: Safe to call concurrently

#### `forgedb_get_relations`

Gets related records.

**Signature**:
```c
char* forgedb_get_relations(
    ForgeDB* db,
    const char* model,
    const char* id,
    const char* relation_name,
    ForgeDBError** error
);
```

**Parameters**:
- `db`: Database handle
- `model`: Model name (e.g., "User")
- `id`: Record ID
- `relation_name`: Name of relation field (e.g., "posts")
- `error`: Output parameter for error

**Returns**:
- JSON array of related records (must be freed)
- NULL on error

**Thread Safety**: Safe to call concurrently

**Example**:
```c
// Get all posts for user 123
char* json = forgedb_get_relations(db, "User", "123", "posts", &err);
if (json != NULL) {
    printf("Posts: %s\n", json);
    forgedb_free_string(json);
}
```

### Memory Management

#### `forgedb_free_string`

Frees a string returned by ForgeDB.

**Signature**:
```c
void forgedb_free_string(char* str);
```

**Parameters**:
- `str`: String to free (can be NULL)

**Behavior**:
- Must be called for every non-NULL string returned by ForgeDB
- Safe to call with NULL (no-op)
- After this call, the string pointer is invalid

**Thread Safety**: Safe to call from any thread

### Error Handling

#### `forgedb_error_code`

Gets error code from error handle.

**Signature**:
```c
int32_t forgedb_error_code(ForgeDBError* error);
```

**Parameters**:
- `error`: Error handle

**Returns**:
- Error code (see Error Codes table)
- `FORGEDB_ERR_INVALID` if error handle is invalid

#### `forgedb_error_message`

Gets error message from error handle.

**Signature**:
```c
const char* forgedb_error_message(ForgeDBError* error);
```

**Parameters**:
- `error`: Error handle

**Returns**:
- Pointer to error message (valid until error is freed)
- "Invalid error handle" if error handle is invalid

**Note**: The returned string is owned by the error object and must not be freed separately.

#### `forgedb_free_error`

Frees an error handle.

**Signature**:
```c
void forgedb_free_error(ForgeDBError* error);
```

**Parameters**:
- `error`: Error handle to free (can be NULL)

**Behavior**:
- Safe to call with NULL (no-op)
- After this call, the error handle is invalid

### Utility

#### `forgedb_version`

Gets ForgeDB version string.

**Signature**:
```c
const char* forgedb_version(void);
```

**Returns**:
- Static string with version number (e.g., "0.1.0")
- Never NULL
- No need to free (static storage)

## Performance Characteristics

### Expected Latency

| Operation | HTTP (Sprint 17) | FFI (Sprint 24) | Improvement |
|-----------|------------------|-----------------|-------------|
| Get single record | 1-2ms | 50-100μs | 10-20x |
| List 10 records | 2-3ms | 100-200μs | 10-15x |
| List 100 records | 5-10ms | 500μs-1ms | 5-10x |
| Get with relations | 3-5ms | 200-300μs | 10-15x |

### Memory Overhead

- **Handle Registry**: ~64 bytes per handle
- **String Returns**: Temporary allocation, freed by caller
- **Error Objects**: ~128 bytes per error
- **Read Lock**: Minimal overhead (RwLock)

### Scaling

- **Concurrent Reads**: Linear scaling with CPU cores
- **Large Result Sets**: O(n) serialization overhead
- **Deep Relations**: Multiple FFI calls required

## Safety Guarantees

### Memory Safety

- No buffer overflows (Rust guarantees)
- No use-after-free (handle registry validation)
- No double-free (handle removed on first close)
- No null pointer dereferences (explicit null checks)

### Thread Safety

- No data races (RwLock protection)
- No deadlocks (read-only, no nested locks)
- No race conditions (atomic handle generation)

### Error Safety

- All errors captured and reported
- No panics across FFI boundary
- Graceful degradation on invalid input

## Limitations

### Current Limitations

1. **Read-Only**: No write operations via FFI
2. **JSON Overhead**: Serialization cost for large datasets
3. **No Transactions**: Each operation is isolated
4. **No Streaming**: Full result set returned at once
5. **String-Based**: Type safety limited to Bun/TypeScript layer

### Future Enhancements

1. Write operations with proper transaction support
2. Binary protocol for zero-copy data access
3. Streaming results for large queries
4. Prepared statements for repeated queries
5. Schema introspection API

## Testing

### Required Tests

- [ ] Memory safety (no leaks)
- [ ] Thread safety (concurrent access)
- [ ] Error handling (all error paths)
- [ ] Performance (vs. HTTP baseline)
- [ ] Resource cleanup (handle lifecycle)

### Tools

- **Valgrind**: Memory leak detection (Linux)
- **Thread Sanitizer**: Race condition detection
- **Criterion**: Performance benchmarking
- **Miri**: Undefined behavior detection

## Changelog

### Version 0.1.0 (2025-10-15)

- Initial specification
- Read-only operations: get, list, query, get_relations
- Handle-based memory management
- JSON string returns
- Thread-safe concurrent reads
