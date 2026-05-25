# Sourced by install.sh and update.sh — sets FEATURES for cargo build.
DESKTOP="${XDG_CURRENT_DESKTOP:-}"
GNOME_EXTENSIONS=$(gnome-extensions list 2>/dev/null || true)

if [[ "$DESKTOP" == *"GNOME"* ]]; then
    if echo "$GNOME_EXTENSIONS" | grep -q "appindicatorsupport@rgcjonas.gmail.com"; then
        echo "GNOME detected with AppIndicator extension — building with SNI tray."
        FEATURES=""
    else
        echo "GNOME detected without AppIndicator extension — building fallback mode."
        echo ""
        echo "⚠  WARNING: the system tray icon will not be available."
        echo "   To get the full experience, install the AppIndicator extension:"
        echo ""
        echo "     sudo apt install gnome-shell-extension-appindicator   # Ubuntu/Debian"
        echo "     sudo dnf install gnome-shell-extension-appindicator   # Fedora"
        echo "     or: https://extensions.gnome.org/extension/615/appindicator-support/"
        echo ""
        echo "   Then log out / back in and re-run this script."
        echo ""
        FEATURES="--no-default-features"
    fi
else
    echo "KDE/other desktop detected — building with SNI tray."
    FEATURES=""
fi
