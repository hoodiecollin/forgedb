# SinkDB Examples

This directory contains example applications demonstrating SinkDB features for each sprint.

## Purpose

After each sprint is completed, we create example applications that:
1. Demonstrate the new features added in that sprint
2. Show how to use the system effectively
3. Highlight improvements made over previous sprints
4. Serve as documentation and testing artifacts

## Structure

Each sprint example includes:
- **`schema.sink`**: Example schema demonstrating sprint features
- **`app.rs`**: Application that parses schema and generates code
- **`client.rs`**: Minimal client showing how to use the generated database
- **`README.md`**: Documentation for the example
- **`generated/`**: Auto-generated database code (created when app runs)

## Available Examples

### Sprint 1: MVP - End-to-End Proof of Concept

**Directory**: `sprint1_mvp/`

**Features Demonstrated**:
- ✅ Schema parsing (simple User model)
- ✅ Code generation (Rust structs and storage)
- ✅ Auto-increment ID (`+u64`)
- ✅ Unique constraints (`&string`)
- ✅ CRUD operations (insert, retrieve)
- ✅ In-memory storage with tombstone bitmap

**Run It**:
```bash
# Generate the database code
cargo run --example sprint1_mvp_app

# Run the client
cargo run --example sprint1_mvp_client
```

**Key Success Criteria**:
- Parse simple schema ✓
- Generate compilable Rust code ✓
- Auto-increment IDs ✓
- Enforce unique constraints ✓
- Retrieve by ID ✓
- No crashes ✓

[See full documentation →](sprint1_mvp/README.md)

---

## Future Sprint Examples

As we complete each sprint, we'll add new examples here:

### Sprint 2: Persistence & Basic Types (Coming Soon)
- Memory-mapped file storage
- Expanded type support (i32, i64, f64, bool, uuid, timestamp)
- Schema validation with helpful errors
- Data persistence across restarts

### Sprint 3: Indexing & Queries (Coming Soon)
- Hash indexes for fast lookups
- Query operations (list, filter, update, delete)
- Index symbol (`^`) support
- Automatic index rebuilding

### Sprint 4: Relations (One-to-Many) (Coming Soon)
- Relation syntax (`posts: [Post]`, `author: *User`)
- Foreign key constraints
- Relation traversal
- Junction table generation

...and more as development progresses!

## Running Examples

### Quick Start

To run any example:
```bash
# Pattern: cargo run --example {sprint_name}_{type}
cargo run --example sprint1_mvp_app      # Generates code
cargo run --example sprint1_mvp_client   # Uses generated code
```

### Understanding the Workflow

Each sprint example follows this pattern:

1. **Define Schema** (`schema.sink`)
   - Describes your data models
   - Uses SinkDB schema syntax

2. **Generate Code** (run `app.rs`)
   - Parses the schema
   - Generates Rust code
   - Creates `generated/database.rs`

3. **Use Database** (run `client.rs`)
   - Imports generated code
   - Demonstrates CRUD operations
   - Tests all sprint features

## Code Organization

```
examples/
├── README.md                    # This file
├── basic.rs                     # Original simple example
└── sprint1_mvp/                 # Sprint 1 MVP example
    ├── README.md                # Sprint 1 documentation
    ├── schema.sink              # Example schema
    ├── app.rs                   # Schema parser & code generator
    ├── client.rs                # Database usage example
    └── generated/               # Auto-generated (gitignored)
        └── database.rs          # Generated database code
```

## Development Notes

### For Contributors

When adding a new sprint example:

1. Create a new directory: `examples/sprint{N}_{feature}/`
2. Include all required files (schema, app, client, README)
3. Update `Cargo.toml` with example entries
4. Update this README with the new example
5. Ensure all examples run successfully

### For Users

- Each example is self-contained
- Generated code is in `generated/` (not committed to git)
- Run the `app` example first to generate code
- Then run the `client` example to see it in action

## Testing

Examples serve as integration tests:
```bash
# Test all examples
cargo test --examples

# Run a specific example
cargo run --example sprint1_mvp_client
```

## Common Issues

### "Failed to read schema file"
**Solution**: Make sure you're running from the project root directory.

### "Module `database` not found"
**Solution**: Run the `_app` example first to generate the code.

### "Unexpected character" in parser
**Solution**: Check your schema syntax. Comments use `//` syntax.

## Next Steps

1. Review the Sprint 1 MVP example
2. Run it to understand the workflow
3. Look at the generated code to see what SinkDB creates
4. Modify the schema and regenerate to experiment
5. Wait for Sprint 2 examples to see persistence in action!

## Questions?

- Check the main [SPRINT_PLAN.md](../SPRINT_PLAN.md) for feature roadmap
- Review individual sprint README files for detailed docs
- Open an issue for bugs or feature requests
