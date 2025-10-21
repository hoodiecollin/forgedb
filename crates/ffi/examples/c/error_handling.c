/**
 * Error Handling Example for ForgeDB FFI
 * 
 * Demonstrates proper error handling patterns including:
 * - Checking for NULL returns
 * - Reading error codes
 * - Reading error messages
 * - Cleaning up error objects
 * 
 * Compile:
 *   gcc -o error_handling error_handling.c -I../../include -L../../../target/release -lforgedb_ffi
 * 
 * Run:
 *   LD_LIBRARY_PATH=../../../target/release ./error_handling
 */

#include "forgedb.h"
#include <stdio.h>
#include <stdlib.h>

void demonstrate_error_handling(void) {
    printf("=== Error Handling Demonstration ===\n\n");
    
    // Example 1: Opening non-existent database
    printf("1. Attempting to open non-existent database...\n");
    ForgeDBError* err = NULL;
    ForgeDB* db = forgedb_open("/nonexistent/path/db", FORGEDB_OPEN_READONLY, &err);
    
    if (db == NULL) {
        if (err != NULL) {
            int code = forgedb_error_code(err);
            const char* msg = forgedb_error_message(err);
            
            printf("   Expected error occurred:\n");
            printf("   Code: %d\n", code);
            printf("   Message: %s\n", msg);
            
            // Check specific error type
            if (code == FORGEDB_ERR_IO) {
                printf("   This is an I/O error (as expected)\n");
            }
            
            forgedb_free_error(err);
        }
    } else {
        printf("   Unexpectedly succeeded!\n");
        forgedb_close(db);
    }
    
    printf("\n");
    
    // Example 2: Invalid handle usage
    printf("2. Using invalid (NULL) database handle...\n");
    err = NULL;
    char* result = forgedb_get(NULL, "User", "123", &err);
    
    if (result == NULL) {
        if (err != NULL) {
            int code = forgedb_error_code(err);
            const char* msg = forgedb_error_message(err);
            
            printf("   Expected error occurred:\n");
            printf("   Code: %d\n", code);
            printf("   Message: %s\n", msg);
            
            if (code == FORGEDB_ERR_INVALID) {
                printf("   This is an invalid parameter error (as expected)\n");
            }
            
            forgedb_free_error(err);
        }
    } else {
        printf("   Unexpectedly succeeded!\n");
        forgedb_free_string(result);
    }
    
    printf("\n");
    
    // Example 3: Ignoring errors (NULL error pointer)
    printf("3. Calling with NULL error pointer (ignoring errors)...\n");
    db = forgedb_open("/nonexistent/path/db", FORGEDB_OPEN_READONLY, NULL);
    
    if (db == NULL) {
        printf("   Operation failed, but no error details available\n");
        printf("   This is valid when you don't need error details\n");
    }
    
    printf("\n");
    
    // Example 4: Error handling with valid database
    printf("4. Opening valid database and handling not-found...\n");
    
    // Try to open a database at ./data
    err = NULL;
    db = forgedb_open("./data", FORGEDB_OPEN_CREATE, &err);
    
    if (db == NULL) {
        printf("   Could not create test database\n");
        if (err != NULL) {
            printf("   Error: %s\n", forgedb_error_message(err));
            forgedb_free_error(err);
        }
        return;
    }
    
    printf("   Database opened successfully\n");
    
    // Try to get a record that likely doesn't exist
    err = NULL;
    result = forgedb_get(db, "User", "99999", &err);
    
    if (result == NULL) {
        if (err != NULL) {
            int code = forgedb_error_code(err);
            const char* msg = forgedb_error_message(err);
            
            if (code == FORGEDB_ERR_NOT_FOUND) {
                printf("   Record not found (this is normal)\n");
            } else {
                printf("   Unexpected error: %s\n", msg);
            }
            
            forgedb_free_error(err);
        }
    } else {
        printf("   Found record: %s\n", result);
        forgedb_free_string(result);
    }
    
    forgedb_close(db);
    printf("\n");
}

void demonstrate_error_switch(void) {
    printf("=== Error Code Switch Statement ===\n\n");
    
    ForgeDBError* err = NULL;
    ForgeDB* db = forgedb_open("/invalid/path", FORGEDB_OPEN_READONLY, &err);
    
    if (db == NULL && err != NULL) {
        int code = forgedb_error_code(err);
        const char* msg = forgedb_error_message(err);
        
        switch (code) {
            case FORGEDB_OK:
                printf("No error (should not happen here)\n");
                break;
                
            case FORGEDB_ERR_IO:
                printf("I/O Error: %s\n", msg);
                printf("Possible causes: file not found, permission denied, disk full\n");
                break;
                
            case FORGEDB_ERR_NOT_FOUND:
                printf("Not Found: %s\n", msg);
                printf("The requested resource does not exist\n");
                break;
                
            case FORGEDB_ERR_INVALID:
                printf("Invalid Parameter: %s\n", msg);
                printf("Check your function arguments\n");
                break;
                
            case FORGEDB_ERR_INTERNAL:
                printf("Internal Error: %s\n", msg);
                printf("This is likely a bug - please report it\n");
                break;
                
            default:
                printf("Unknown error code %d: %s\n", code, msg);
                break;
        }
        
        forgedb_free_error(err);
    }
    
    printf("\n");
}

int main(void) {
    printf("ForgeDB FFI Error Handling Examples\n");
    printf("====================================\n\n");
    
    demonstrate_error_handling();
    demonstrate_error_switch();
    
    printf("All error handling examples completed successfully!\n");
    return 0;
}
