#!/bin/bash
# Sprint 5: Dev Mode Demo
# Demonstrates the `sinkdb dev` command with live schema watching

set -e

echo "Sprint 5: Dev Mode Demo"
echo "======================="
echo ""

# Create temporary test directory
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

echo "📁 Working directory: $TEST_DIR"
echo ""

# Create initial schema
cat > schema.sink << 'EOF'
User {
  id: +uuid
  email: ^&string
  name: string
}
EOF

echo "✓ Created initial schema (User model)"
echo ""
echo "Starting dev mode (will watch for changes)..."
echo "Press Ctrl+C to stop"
echo ""
echo "────────────────────────────────────────────────"
echo ""

# Start the dev command
# In a real scenario, this would run indefinitely
# For demo purposes, you can run this script and manually
# edit schema.sink to see live regeneration

sinkdb dev --schema schema.sink --output generated

# Cleanup (only reached if Ctrl+C)
echo ""
echo "Cleaning up..."
cd -
rm -rf "$TEST_DIR"
