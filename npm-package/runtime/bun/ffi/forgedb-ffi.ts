// ForgeDB FFI Bindings for Bun
// Provides low-level access to ForgeDB via C FFI

import { dlopen, FFIType, suffix, ptr } from "bun:ffi";
import { join } from "path";

// Find the shared library
function findLibrary(): string {
  const libName = `libforgedb_ffi.${suffix}`;

  // Get the project root (3 levels up from runtime/bun/ffi)
  const projectRoot = join(import.meta.dir, "..", "..", "..");

  // Check common locations
  const locations = [
    join(projectRoot, "target", "release", libName),
    join(projectRoot, "target", "debug", libName),
    join(projectRoot, "lib", libName),
    join(process.cwd(), "target", "release", libName),
    libName, // System path
  ];

  for (const location of locations) {
    try {
      // Try to open each location
      const lib = dlopen(location, {
        forgedb_version: {
          args: [],
          returns: FFIType.cstring,
        },
      });
      lib.close();
      return location;
    } catch {
      continue;
    }
  }

  throw new Error(`Could not find ${libName} in: ${locations.join(", ")}`);
}

// FFI symbol declarations
const lib = dlopen(findLibrary(), {
  // Version
  forgedb_version: {
    args: [],
    returns: FFIType.cstring,
  },

  // Database lifecycle
  forgedb_open: {
    args: [FFIType.cstring, FFIType.i32, FFIType.ptr],
    returns: FFIType.ptr,
  },
  forgedb_close: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },

  // Read operations (return ptr so we can manually free)
  forgedb_get: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.ptr],
    returns: FFIType.ptr,
  },
  forgedb_list: {
    args: [
      FFIType.ptr,
      FFIType.cstring,
      FFIType.ptr,
      FFIType.i32,
      FFIType.i32,
      FFIType.ptr,
    ],
    returns: FFIType.ptr,
  },
  forgedb_query: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.cstring, FFIType.ptr],
    returns: FFIType.ptr,
  },
  forgedb_get_relations: {
    args: [
      FFIType.ptr,
      FFIType.cstring,
      FFIType.cstring,
      FFIType.cstring,
      FFIType.ptr,
    ],
    returns: FFIType.ptr,
  },

  // Memory management
  forgedb_free_string: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },

  // Error handling
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
});

export const { symbols } = lib;

// Export types
export type ForgeDBHandle = number;
export type ForgeDBErrorHandle = number;

// Constants
export const FORGEDB_OPEN_READONLY = 0x01;
export const FORGEDB_OPEN_CREATE = 0x02;

export const FORGEDB_OK = 0;
export const FORGEDB_ERR_IO = 1;
export const FORGEDB_ERR_NOT_FOUND = 2;
export const FORGEDB_ERR_INVALID = 3;
export const FORGEDB_ERR_INTERNAL = 4;
