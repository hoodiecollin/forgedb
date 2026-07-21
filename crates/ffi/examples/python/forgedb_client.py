"""
ForgeDB Python Wrapper

A high-level, Pythonic wrapper for the ForgeDB FFI.

Usage:
    from forgedb_client import ForgeDBClient
    
    with ForgeDBClient("./data") as db:
        user = db.get("User", 123)
        users = db.list("User", limit=10)
"""

import ctypes
import json
import os
import platform
from typing import Any, Dict, List, Optional

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
_lib_path = get_library_path()
_forgedb = ctypes.CDLL(_lib_path)

# Define opaque handle types
class _ForgeDB(ctypes.Structure):
    pass

class _ForgeDBError(ctypes.Structure):
    pass

# Define function signatures
_forgedb.forgedb_version.argtypes = []
_forgedb.forgedb_version.restype = ctypes.c_char_p

_forgedb.forgedb_open.argtypes = [
    ctypes.c_char_p, 
    ctypes.c_int, 
    ctypes.POINTER(ctypes.POINTER(_ForgeDBError))
]
_forgedb.forgedb_open.restype = ctypes.POINTER(_ForgeDB)

_forgedb.forgedb_close.argtypes = [ctypes.POINTER(_ForgeDB)]
_forgedb.forgedb_close.restype = None

_forgedb.forgedb_get.argtypes = [
    ctypes.POINTER(_ForgeDB), 
    ctypes.c_char_p, 
    ctypes.c_char_p, 
    ctypes.POINTER(ctypes.POINTER(_ForgeDBError))
]
_forgedb.forgedb_get.restype = ctypes.c_char_p

_forgedb.forgedb_list.argtypes = [
    ctypes.POINTER(_ForgeDB),
    ctypes.c_char_p,
    ctypes.c_char_p,
    ctypes.c_int32,
    ctypes.c_int32,
    ctypes.POINTER(ctypes.POINTER(_ForgeDBError))
]
_forgedb.forgedb_list.restype = ctypes.c_char_p

_forgedb.forgedb_query.argtypes = [
    ctypes.POINTER(_ForgeDB),
    ctypes.c_char_p,
    ctypes.c_char_p,
    ctypes.POINTER(ctypes.POINTER(_ForgeDBError))
]
_forgedb.forgedb_query.restype = ctypes.c_char_p

_forgedb.forgedb_error_code.argtypes = [ctypes.POINTER(_ForgeDBError)]
_forgedb.forgedb_error_code.restype = ctypes.c_int32

_forgedb.forgedb_error_message.argtypes = [ctypes.POINTER(_ForgeDBError)]
_forgedb.forgedb_error_message.restype = ctypes.c_char_p

_forgedb.forgedb_free_error.argtypes = [ctypes.POINTER(_ForgeDBError)]
_forgedb.forgedb_free_error.restype = None

_forgedb.forgedb_free_string.argtypes = [ctypes.c_char_p]
_forgedb.forgedb_free_string.restype = None

# Constants
_FORGEDB_OPEN_READONLY = 0x01
_FORGEDB_OPEN_CREATE = 0x02
_FORGEDB_ERR_NOT_FOUND = 2


class ForgeDBError(Exception):
    """Exception raised for ForgeDB errors"""
    def __init__(self, code: int, message: str):
        self.code = code
        self.message = message
        super().__init__(f"ForgeDB Error {code}: {message}")


class ForgeDBClient:
    """
    High-level Python wrapper for ForgeDB FFI.
    
    Example:
        with ForgeDBClient("./data") as db:
            user = db.get("User", 123)
            users = db.list("User", limit=10)
    """
    
    def __init__(self, path: str, readonly: bool = True):
        """
        Open a ForgeDB database.
        
        Args:
            path: Path to the database directory
            readonly: If True, open in read-only mode (default: True)
        
        Raises:
            ForgeDBError: If the database cannot be opened
        """
        self.db = None
        self.path = path
        flags = _FORGEDB_OPEN_READONLY if readonly else _FORGEDB_OPEN_CREATE
        
        err = ctypes.POINTER(_ForgeDBError)()
        self.db = _forgedb.forgedb_open(path.encode('utf-8'), flags, ctypes.byref(err))
        
        if not self.db:
            msg = _forgedb.forgedb_error_message(err)
            code = _forgedb.forgedb_error_code(err)
            _forgedb.forgedb_free_error(err)
            raise ForgeDBError(code, msg.decode('utf-8'))
    
    def __enter__(self):
        """Context manager entry"""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - ensures database is closed"""
        self.close()
    
    def close(self):
        """Close the database connection"""
        if self.db:
            _forgedb.forgedb_close(self.db)
            self.db = None
    
    def get(self, model: str, id: Any) -> Optional[Dict[str, Any]]:
        """
        Get a single record by ID.
        
        Args:
            model: Model name (e.g., "User")
            id: Record ID
        
        Returns:
            Dictionary with record data, or None if not found
        
        Raises:
            ForgeDBError: If an error occurs (other than not found)
        """
        if not self.db:
            raise ForgeDBError(3, "Database is closed")
        
        err = ctypes.POINTER(_ForgeDBError)()
        json_str = _forgedb.forgedb_get(
            self.db, 
            model.encode('utf-8'), 
            str(id).encode('utf-8'), 
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            _forgedb.forgedb_free_string(json_str)
            return result
        
        if err:
            code = _forgedb.forgedb_error_code(err)
            msg = _forgedb.forgedb_error_message(err)
            _forgedb.forgedb_free_error(err)
            
            if code == _FORGEDB_ERR_NOT_FOUND:
                return None
            raise ForgeDBError(code, msg.decode('utf-8'))
        
        return None
    
    def list(
        self, 
        model: str, 
        limit: int = 0, 
        offset: int = 0,
        filters: Optional[Dict[str, Any]] = None
    ) -> List[Dict[str, Any]]:
        """
        List records with optional filtering and pagination.
        
        Args:
            model: Model name (e.g., "User")
            limit: Maximum number of records to return (0 for all)
            offset: Number of records to skip (0 for none)
            filters: Optional dictionary of filter conditions
        
        Returns:
            List of dictionaries with record data
        
        Raises:
            ForgeDBError: If an error occurs
        """
        if not self.db:
            raise ForgeDBError(3, "Database is closed")
        
        filter_json = None
        if filters:
            filter_json = json.dumps(filters).encode('utf-8')
        
        err = ctypes.POINTER(_ForgeDBError)()
        json_str = _forgedb.forgedb_list(
            self.db,
            model.encode('utf-8'),
            filter_json,
            limit,
            offset,
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            _forgedb.forgedb_free_string(json_str)
            return result
        
        if err:
            msg = _forgedb.forgedb_error_message(err)
            code = _forgedb.forgedb_error_code(err)
            _forgedb.forgedb_free_error(err)
            raise ForgeDBError(code, msg.decode('utf-8'))
        
        return []
    
    def query(self, model: str, query: Dict[str, Any]) -> List[Dict[str, Any]]:
        """
        Execute a complex query.
        
        Args:
            model: Model name (e.g., "User")
            query: Query specification with filters, limit, offset, etc.
        
        Returns:
            List of dictionaries with record data
        
        Raises:
            ForgeDBError: If an error occurs
        """
        if not self.db:
            raise ForgeDBError(3, "Database is closed")
        
        err = ctypes.POINTER(_ForgeDBError)()
        query_json = json.dumps(query).encode('utf-8')
        
        json_str = _forgedb.forgedb_query(
            self.db,
            model.encode('utf-8'),
            query_json,
            ctypes.byref(err)
        )
        
        if json_str:
            result = json.loads(json_str.decode('utf-8'))
            _forgedb.forgedb_free_string(json_str)
            return result
        
        if err:
            msg = _forgedb.forgedb_error_message(err)
            code = _forgedb.forgedb_error_code(err)
            _forgedb.forgedb_free_error(err)
            raise ForgeDBError(code, msg.decode('utf-8'))
        
        return []
    
    @staticmethod
    def version() -> str:
        """Get the ForgeDB FFI version string"""
        return _forgedb.forgedb_version().decode('utf-8')


# Example usage
if __name__ == "__main__":
    print(f"ForgeDB FFI version: {ForgeDBClient.version()}")
    
    try:
        with ForgeDBClient("./data") as db:
            # Get a user
            user = db.get("User", 123)
            if user:
                print(f"Found user: {user}")
            else:
                print("User not found")
            
            # List users
            users = db.list("User", limit=10)
            print(f"Found {len(users)} users")
            
            # Query users
            results = db.query("User", {"limit": 5, "offset": 0})
            print(f"Query returned {len(results)} results")
    
    except ForgeDBError as e:
        print(f"Database error: {e}")
