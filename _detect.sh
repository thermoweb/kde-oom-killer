# Sourced by install.sh and update.sh — sets FEATURES for cargo build.
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
