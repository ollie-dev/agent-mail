#!/bin/sh
# Serve the factory dashboard at http://localhost:<port>/dashboard.html
# Usage: factory/serve.sh [port]   (default port 8000)
set -eu

port="${1:-8000}"
cd "$(dirname "$0")"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 not found; install Python 3 to serve the dashboard." >&2
  exit 1
fi

echo "Dashboard: http://localhost:${port}/dashboard.html"
echo "Stop with Ctrl-C."
exec python3 -m http.server "$port"
