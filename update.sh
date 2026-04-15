#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="rambo"
INSTALL_DIR="${HOME}/.local/bin"

echo "🛑 Stopping ${BINARY_NAME}…"
systemctl --user stop "${BINARY_NAME}.service"

echo "🔨 Building ${BINARY_NAME}…"
cargo build --release

echo "📦 Updating binary in ${INSTALL_DIR}/"
cp "target/release/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

echo "🚀 Restarting the service…"
systemctl --user start "${BINARY_NAME}.service"

echo ""
echo "✅ Done! rambo updated and running."
echo ""
echo "  systemctl --user status rambo   # check status"
echo "  journalctl --user -u rambo -f   # follow logs"
