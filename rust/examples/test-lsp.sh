#!/bin/bash
# Simple LSP test script
# This sends a basic initialize request to test the LSP server

set -e

MANTRA_BIN="${1:-./target/debug/mantra}"

echo "Testing mantra LSP server..."
echo "Binary: $MANTRA_BIN"

# Create a test request (JSON-RPC initialize)
REQUEST=$(cat <<'EOF'
Content-Length: 147

{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":"file:///tmp","capabilities":{}}}
EOF
)

# Send request and capture response
echo "$REQUEST" | RUST_LOG=mantra=debug "$MANTRA_BIN" 2>&1 | head -20

echo ""
echo "If you see 'LSP initialize request received' in the logs, the server is working!"
