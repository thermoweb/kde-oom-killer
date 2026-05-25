#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="rambo"
INSTALL_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
SERVICE_FILE="${SERVICE_DIR}/rambo.service"

DESKTOP="${XDG_CURRENT_DESKTOP:-}"
GNOME_EXTENSIONS=$(gnome-extensions list 2>/dev/null || true)

if [[ "$DESKTOP" == *"GNOME"* ]]; then
    if echo "$GNOME_EXTENSIONS" | grep -q "appindicatorsupport@rgcjonas.gmail.com"; then
        echo "GNOME detected with AppIndicator extension — building with SNI tray."
        FEATURES=""
    else
        echo "GNOME detected without AppIndicator extension — building fallback mode."
        echo "Tip: install https://extensions.gnome.org/extension/615/appindicator-support/ for a tray icon."
        FEATURES="--no-default-features"
    fi
else
    echo "KDE/other desktop detected — building with SNI tray."
    FEATURES=""
fi

echo "🔨 Building ${BINARY_NAME}…"
cargo build --release $FEATURES

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
