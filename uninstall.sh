#!/usr/bin/env bash
set -euo pipefail

echo "🛑 Stopping and disabling rambo…"
systemctl --user stop rambo.service 2>/dev/null || true
systemctl --user disable rambo.service 2>/dev/null || true
rm -f "${HOME}/.config/systemd/user/rambo.service"
systemctl --user daemon-reload
rm -f "${HOME}/.local/bin/rambo"

echo "✅ rambo uninstalled."
echo "   Config preserved at: ${HOME}/.config/rambo/config.json"
echo "   Remove manually if desired: rm -rf ~/.config/rambo"
