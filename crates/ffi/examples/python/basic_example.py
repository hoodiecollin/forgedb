"""
Basic ForgeDB FFI Example in Python

Demonstrates using ForgeDB from Python via ctypes.

Usage:
    python basic_example.py [database_path]
"""

import ctypes
import json
import os
import sys
import platform

# Determine library path based on OS
def get_library_path():
    system = platform.system()
    base_path = "../../../target/release/"
    
    if system == "Linux":
        return os.path.join(base_path, "libforgedb_ffi.so")
    elif system == "Darwin":  # macOS
        return os.path.join(base_path, "libforgedb_ffi.dylib")
    elif system == "Windows":
        return os.path.join(base_path, "forgedb_ffi.dll")
    else:
        raise Exception(f"Unsupported platform: {system}")

# Load the library
lib_path = get_library_path()
if not os.path.exists(lib_path):
    print(f"Error: Library not found at {lib_path}")
    print("Please build the library first with:")
    print("  cargo build -p forgedb-ffi --release")
    sys.exit(1)

forgedb = ctypes.CDLL(lib_path)

# Define opaque handle types
class ForgeDB(ctypes.Structure):
    pass

class ForgeDBError(ctypes.Structure):
    pass

# Define function signatures
forgedb.forgedb_version.argtypes = []
forgedb.forgedb_version.restype = ctypes.c_char_p

forgedb.forgedb_open.argtypes = [
    ctypes.c_char_p, 
    ctypes.c_int, 
    ctypes.POINTER(ctypes.POINTER(ForgeDBError))
]
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

forgedb.forgedb_query.argtypes = [
    ctypes.POINTER(ForgeDB),
    ctypes.c_char_p,
    ctypes.c_char_p,
    ctypes.POINTER(ctypes.POINTER(ForgeDBError))
]
forgedb.forgedb_query.restype = ctypes.c_char_p

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

def main():
    db_path = sys.argv[1] if len(sys.argv) > 1 else "./data"
    
    # Print version
    version = forgedb.forgedb_version()
    print(f"ForgeDB FFI version: {version.decode('utf-8')}")
    print(f"Opening database at: {db_path}\n")
    
    # Open database
    err = ctypes.POINTER(ForgeDBError)()
    db = forgedb.forgedb_open(
        db_path.encode('utf-8'), 
        FORGEDB_OPEN_CREATE, 
        ctypes.byref(err)
    )
    
    if not db:
        msg = forgedb.forgedb_error_message(err)
        print(f"Failed to open database: {msg.decode('utf-8')}")
        forgedb.forgedb_free_error(err)
        return 1
    
    print("Database opened successfully\n")
    
    try:
        # Get a specific record
        print("Getting user with ID 123...")
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_get(db, b"User", b"123", ctypes.byref(err))
        
        if json_str:
            user = json.loads(json_str.decode('utf-8'))
            print(f"User 123: {json.dumps(user, indent=2)}\n")
            forgedb.forgedb_free_string(json_str)
        elif err:
            code = forgedb.forgedb_error_code(err)
            if code == FORGEDB_ERR_NOT_FOUND:
                print("User 123 not found\n")
            else:
                msg = forgedb.forgedb_error_message(err)
                print(f"Error: {msg.decode('utf-8')}\n")
            forgedb.forgedb_free_error(err)
        
        # List all records
        print("Listing all users...")
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_list(db, b"User", None, 0, 0, ctypes.byref(err))
        
        if json_str:
            users = json.loads(json_str.decode('utf-8'))
            print(f"Found {len(users)} users")
            if users:
                print(f"First user: {json.dumps(users[0], indent=2)}\n")
            else:
                print("(empty)\n")
            forgedb.forgedb_free_string(json_str)
        else:
            msg = forgedb.forgedb_error_message(err)
            print(f"Error: {msg.decode('utf-8')}\n")
            forgedb.forgedb_free_error(err)
        
        # List with pagination
        print("Listing first 5 users...")
        err = ctypes.POINTER(ForgeDBError)()
        json_str = forgedb.forgedb_list(db, b"User", None, 5, 0, ctypes.byref(err))
        
        if json_str:
            users = json.loads(json_str.decode('utf-8'))
            print(f"Found {len(users)} users (page 1)")
            forgedb.forgedb_free_string(json_str)
        else:
            msg = forgedb.forgedb_error_message(err)
            print(f"Error: {msg.decode('utf-8')}")
            forgedb.forgedb_free_error(err)
        
        print()
        
        # Query with JSON
        print("Querying users with limit 10...")
        err = ctypes.POINTER(ForgeDBError)()
        query = json.dumps({"limit": 10, "offset": 0})
        json_str = forgedb.forgedb_query(
            db, 
            b"User", 
            query.encode('utf-8'), 
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            print(f"Query returned {len(result)} users\n")
            forgedb.forgedb_free_string(json_str)
        else:
            msg = forgedb.forgedb_error_message(err)
            print(f"Error: {msg.decode('utf-8')}\n")
            forgedb.forgedb_free_error(err)
    
    finally:
        # Close database
        print("Closing database...")
        forgedb.forgedb_close(db)
        print("Done!")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
