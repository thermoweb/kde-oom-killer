# Sourced by install.sh and update.sh — sets FEATURES for cargo build.
DESKTOP="${XDG_CURRENT_DESKTOP:-}"

# Extensions that provide a StatusNotifierItem / AppIndicator host on GNOME.
# Either one is enough for the SNI tray icon to appear. Ubuntu ships its own
# fork (ubuntu-appindicators@ubuntu.com) enabled by default.
APPINDICATOR_UUIDS=(
    "appindicatorsupport@rgcjonas.gmail.com"
    "ubuntu-appindicators@ubuntu.com"
)

print_install_help() {
    echo "   To get the system tray icon, install the AppIndicator extension:"
    echo ""
    echo "     sudo apt install gnome-shell-extension-appindicator   # Ubuntu/Debian"
    echo "     sudo dnf install gnome-shell-extension-appindicator   # Fedora"
    echo "     or: https://extensions.gnome.org/extension/615/appindicator-support/"
    echo ""
    echo "   Then log out / back in and re-run this script."
}

if [[ "$DESKTOP" == *"GNOME"* ]]; then
    ENABLED=$(gnome-extensions list --enabled 2>/dev/null || true)
    INSTALLED=$(gnome-extensions list 2>/dev/null || true)

    enabled_uuid=""
    installed_uuid=""
    for uuid in "${APPINDICATOR_UUIDS[@]}"; do
        if echo "$ENABLED" | grep -qx "$uuid"; then enabled_uuid="$uuid"; fi
        if echo "$INSTALLED" | grep -qx "$uuid"; then installed_uuid="$uuid"; fi
    done

    if [[ -n "$enabled_uuid" ]]; then
        echo "GNOME detected with AppIndicator host ($enabled_uuid) — building with SNI tray."
        FEATURES=""
    elif [[ -n "$installed_uuid" ]]; then
        echo "GNOME: AppIndicator extension ($installed_uuid) is installed but NOT enabled — enabling it…"
        if gnome-extensions enable "$installed_uuid" 2>/dev/null; then
            echo "Enabled. Building with SNI tray (log out / back in if the icon doesn't appear)."
        else
            echo "⚠  Could not enable automatically — building with SNI tray anyway."
            echo "   Enable it manually: gnome-extensions enable $installed_uuid"
        fi
        FEATURES=""
    else
        echo "GNOME detected without any AppIndicator host extension — building fallback mode."
        echo ""
        echo "⚠  WARNING: the system tray icon will not be available."
        echo ""
        print_install_help
        echo ""
        FEATURES="--no-default-features"
    fi
else
    echo "KDE/other desktop detected — building with SNI tray."
    FEATURES=""
fi
