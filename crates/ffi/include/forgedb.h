#ifndef FORGEDB_FFI_H
#define FORGEDB_FFI_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Get ForgeDB version string
 *
 * Returns a static string with the version number.
 * No need to free (static storage).
 *
 * # Safety
 *
 * This function is safe to call from any thread. The returned pointer
 * points to static storage and must not be freed.
 *
 * # Returns
 *
 * Pointer to null-terminated version string (e.g., "0.1.0")
 */
const char *forgedb_version(void);

#endif /* FORGEDB_FFI_H */
