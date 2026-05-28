#!/bin/bash
export PATH="/c/msys64/mingw64/bin:$HOME/.cargo/bin:$PATH"
export COEVO_DATABASE_URL="sqlite:C:/coevo-build/coevo/data/coevo.db?mode=rwc"
cd /c/coevo-build/coevo
cargo run -p coevo-server > /c/coevo-build/coevo/server.log 2>&1 &
SERVER_PID=$!
echo "Server started with PID: $SERVER_PID"

# Wait for server to start
for i in $(seq 1 30); do
  if grep -q "listening" /c/coevo-build/coevo/server.log 2>/dev/null; then
    echo "Server is ready!"
    break
  fi
  sleep 1
done

echo "=== SERVER LOG ==="
cat /c/coevo-build/coevo/server.log
echo "=== END SERVER LOG ==="

echo ""
echo "=== CURL health ==="
curl -s http://127.0.0.1:8717/health
echo ""
echo "=== CURL openapi.json ==="
curl -s http://127.0.0.1:8717/openapi.json | head -c 500
echo ""
