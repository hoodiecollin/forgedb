# Makefile for ForgeDB FFI C Examples

CC = gcc
CFLAGS = -Wall -Wextra -I../../include
LDFLAGS = -L../../../target/release -lforgedb_ffi

# Detect OS for library path
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
    LIB_PATH = LD_LIBRARY_PATH=../../../target/release
endif
ifeq ($(UNAME_S),Darwin)
    LIB_PATH = DYLD_LIBRARY_PATH=../../../target/release
endif

EXAMPLES = basic_example error_handling

.PHONY: all clean run

all: $(EXAMPLES)

basic_example: basic_example.c
	$(CC) $(CFLAGS) -o $@ $< $(LDFLAGS)

error_handling: error_handling.c
	$(CC) $(CFLAGS) -o $@ $< $(LDFLAGS)

run: all
	@echo "Running basic_example..."
	$(LIB_PATH) ./basic_example
	@echo ""
	@echo "Running error_handling..."
	$(LIB_PATH) ./error_handling

clean:
	rm -f $(EXAMPLES)
