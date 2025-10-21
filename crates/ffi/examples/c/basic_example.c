/**
 * Basic ForgeDB FFI Example in C
 * 
 * Demonstrates opening a database, reading records, and proper memory management.
 * 
 * Compile:
 *   gcc -o basic_example basic_example.c -I../../include -L../../../target/release -lforgedb_ffi
 * 
 * Run:
 *   LD_LIBRARY_PATH=../../../target/release ./basic_example
 */

#include "forgedb.h"
#include <stdio.h>
#include <stdlib.h>

void print_error(ForgeDBError* err) {
    if (err != NULL) {
        int code = forgedb_error_code(err);
        const char* msg = forgedb_error_message(err);
        fprintf(stderr, "Error %d: %s\n", code, msg);
        forgedb_free_error(err);
    }
}

int main(int argc, char* argv[]) {
    const char* db_path = argc > 1 ? argv[1] : "./data";
    
    printf("ForgeDB FFI version: %s\n", forgedb_version());
    printf("Opening database at: %s\n", db_path);
    
    // Open database
    ForgeDBError* err = NULL;
    ForgeDB* db = forgedb_open(db_path, FORGEDB_OPEN_READONLY, &err);
    
    if (db == NULL) {
        fprintf(stderr, "Failed to open database at %s\n", db_path);
        print_error(err);
        return 1;
    }
    
    printf("Database opened successfully\n\n");
    
    // Get a specific record
    printf("Getting user with ID 123...\n");
    err = NULL;
    char* json = forgedb_get(db, "User", "123", &err);
    
    if (json != NULL) {
        printf("User 123: %s\n\n", json);
        forgedb_free_string(json);
    } else if (err != NULL) {
        int code = forgedb_error_code(err);
        if (code == FORGEDB_ERR_NOT_FOUND) {
            printf("User 123 not found\n\n");
        } else {
            print_error(err);
        }
        err = NULL;
    }
    
    // List all users
    printf("Listing all users...\n");
    err = NULL;
    char* users = forgedb_list(db, "User", NULL, 0, 0, &err);
    
    if (users != NULL) {
        printf("Users: %s\n\n", users);
        forgedb_free_string(users);
    } else {
        print_error(err);
    }
    
    // List users with pagination
    printf("Listing first 5 users...\n");
    err = NULL;
    char* users_page = forgedb_list(db, "User", NULL, 5, 0, &err);
    
    if (users_page != NULL) {
        printf("Users (page 1): %s\n\n", users_page);
        forgedb_free_string(users_page);
    } else {
        print_error(err);
    }
    
    // Query with JSON
    printf("Querying users with limit 10...\n");
    err = NULL;
    char* query_result = forgedb_query(db, "User", "{\"limit\": 10, \"offset\": 0}", &err);
    
    if (query_result != NULL) {
        printf("Query result: %s\n\n", query_result);
        forgedb_free_string(query_result);
    } else {
        print_error(err);
    }
    
    // Close database
    printf("Closing database...\n");
    forgedb_close(db);
    
    printf("Done!\n");
    return 0;
}
