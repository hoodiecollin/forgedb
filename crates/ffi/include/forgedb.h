#ifndef FORGEDB_FFI_H
#define FORGEDB_FFI_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define FORGEDB_OPEN_READONLY 1

#define FORGEDB_OPEN_CREATE 2

#define FORGEDB_OK 0

#define FORGEDB_ERR_IO 1

#define FORGEDB_ERR_NOT_FOUND 2

#define FORGEDB_ERR_INVALID 3

#define FORGEDB_ERR_INTERNAL 4

/**
 * Opaque database handle
 */
typedef struct ForgeDB {
  uint8_t _private[0];
} ForgeDB;

/**
 * Opaque error handle
 */
typedef struct ForgeDBError {
  uint8_t _private[0];
} ForgeDBError;

/**
 * Get ForgeDB version string
 *
 * Returns a static string with the version number.
 * No need to free (static storage).
 */
const char *forgedb_version(void);

/**
 * Open a ForgeDB database
 *
 * # Parameters
 * - `path`: Path to database directory (null-terminated C string)
 * - `flags`: Bitwise OR of FORGEDB_OPEN_* flags
 * - `error`: Output parameter for error (can be NULL)
 *
 * # Returns
 * - Non-NULL handle on success
 * - NULL on error (check error parameter)
 */
struct ForgeDB *forgedb_open(const char *path, int flags, struct ForgeDBError **error);

/**
 * Close a ForgeDB database
 *
 * After this call, the handle is invalid and must not be used.
 * Safe to call with NULL or already-closed handle.
 */
void forgedb_close(struct ForgeDB *db);

/**
 * Get a single record by ID
 *
 * # Parameters
 * - `db`: Database handle
 * - `model`: Model name (currently ignored, only "User" supported)
 * - `id`: Record ID as string
 * - `error`: Output parameter for error (can be NULL)
 *
 * # Returns
 * - JSON string on success (must be freed with forgedb_free_string)
 * - NULL on error or not found
 */
char *forgedb_get(struct ForgeDB *db,
                  const char *_model,
                  const char *id,
                  struct ForgeDBError **error);

/**
 * List records with optional filtering
 *
 * # Parameters
 * - `db`: Database handle
 * - `model`: Model name (currently ignored)
 * - `filter_json`: JSON object with filters (currently ignored, returns all)
 * - `limit`: Maximum number of records (0 for all)
 * - `offset`: Number of records to skip (0 for none)
 * - `error`: Output parameter for error
 *
 * # Returns
 * - JSON array string on success (must be freed)
 * - NULL on error
 */
char *forgedb_list(struct ForgeDB *db,
                   const char *_model,
                   const char *_filter_json,
                   int32_t limit,
                   int32_t offset,
                   struct ForgeDBError **error);

/**
 * Execute complex query (simplified version)
 *
 * Currently just delegates to list with limit/offset from query JSON
 */
char *forgedb_query(struct ForgeDB *db,
                    const char *model,
                    const char *query_json,
                    struct ForgeDBError **error);

/**
 * Get related records (not implemented yet for simple User model)
 *
 * Returns empty array for now
 */
char *forgedb_get_relations(struct ForgeDB *db,
                            const char *_model,
                            const char *_id,
                            const char *_relation_name,
                            struct ForgeDBError **error);

/**
 * Get error code from error handle
 */
int32_t forgedb_error_code(struct ForgeDBError *error);

/**
 * Get error message from error handle
 *
 * Returns a pointer to internal storage. Valid until error is freed.
 */
const char *forgedb_error_message(struct ForgeDBError *error);

/**
 * Free an error handle
 */
void forgedb_free_error(struct ForgeDBError *error);

/**
 * Free a C string returned by ForgeDB
 */
void forgedb_free_string(char *ptr);

#endif /* FORGEDB_FFI_H */
