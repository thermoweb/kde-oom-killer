#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="rambo"
INSTALL_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
SERVICE_FILE="${SERVICE_DIR}/rambo.service"

echo "🔨 Building ${BINARY_NAME}…"
cargo build --release

echo "📦 Installing binary to ${INSTALL_DIR}/"
mkdir -p "${INSTALL_DIR}"
cp "target/release/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

echo "⚙️  Installing systemd user service…"
mkdir -p "${SERVICE_DIR}"
cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=rambo — proactive memory-pressure killer
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/${BINARY_NAME}
Restart=on-failure
RestartSec=5s
Environment=DISPLAY=:0
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus

[Install]
WantedBy=graphical-session.target
EOF

echo "🚀 Enabling and starting the service…"
systemctl --user daemon-reload
systemctl --user enable --now "${BINARY_NAME}.service"

echo ""
echo "✅ Done! rambo is running."
echo ""
echo "Config file: ${HOME}/.config/rambo/config.toml"
echo ""
echo "Useful commands:"
echo "  systemctl --user status rambo   # check status"
echo "  journalctl --user -u rambo -f   # follow logs"
echo "  systemctl --user stop rambo     # stop"
echo "  systemctl --user disable rambo  # disable at login"
