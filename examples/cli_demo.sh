#!/bin/bash
# Sprint 5 CLI Demo Script
# This script demonstrates the SinkDB CLI functionality

set -e

echo "======================================"
echo "SinkDB CLI Demo - Sprint 5"
echo "======================================"
echo ""

# Build the CLI
echo "📦 Building SinkDB CLI..."
cargo build -p sinkdb-cli --release --quiet
echo "✓ CLI built successfully"
echo ""

# Show CLI help
echo "======================================"
echo "1. CLI Help"
echo "======================================"
./target/release/sinkdb --help
echo ""

# Show init command help
echo "======================================"
echo "2. Init Command Help"
echo "======================================"
./target/release/sinkdb init --help
echo ""

# Initialize a test project
echo "======================================"
echo "3. Initialize Test Project"
echo "======================================"
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"
echo "Working in: $TEMP_DIR"
echo ""

../target/release/sinkdb init test-blog --template blog
echo ""

# Show created structure
echo "======================================"
echo "4. Project Structure"
echo "======================================"
cd test-blog
echo "Created files:"
ls -la
echo ""

# Show schema content
echo "======================================"
echo "5. Generated Schema"
echo "======================================"
cat schema.sink
echo ""

# Validate the schema
echo "======================================"
echo "6. Validate Schema"
echo "======================================"
../../../target/release/sinkdb validate
echo ""

# Generate code
echo "======================================"
echo "7. Generate Code"
echo "======================================"
../../../target/release/sinkdb generate
echo ""

# Show generated code snippet
echo "======================================"
echo "8. Generated Code (first 50 lines)"
echo "======================================"
head -n 50 generated/database.rs
echo "... (truncated)"
echo ""

# Clean up
echo "======================================"
echo "9. Cleanup"
echo "======================================"
cd /
rm -rf "$TEMP_DIR"
echo "✓ Temporary files cleaned up"
echo ""

echo "======================================"
echo "Demo Complete!"
echo "======================================"
echo ""
echo "Summary:"
echo "- ✓ CLI built and functional"
echo "- ✓ Project initialization works"
echo "- ✓ Template system works"
echo "- ✓ Schema validation works"
echo "- ✓ Code generation works"
echo ""
echo "Next steps:"
echo "  1. Try: ./target/release/sinkdb init my-project"
echo "  2. Then: cd my-project && cargo run"
