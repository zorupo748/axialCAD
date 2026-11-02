#!/usr/bin/env bash
# install.sh — Installs or updates CADCraft for the current Linux user.
#
# Default installation prefix: $HOME/.local (no root privileges needed)
#
# Usage:
#   ./install.sh [OPTIONS]
#
# Options:
#   --prefix <path>     Target installation directory (default: $HOME/.local)
#   --destdir <path>    Staging root directory for packaging/sandboxing
#   --update            Update an existing installation (preserves configs and documents)
#   --force             Proceed even if CADCraft is running or on collision override
#   --dry-run           Display actions without modifying files
#   --non-interactive   Run without interactive confirmation prompts
#   -h, --help          Show this help message
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source directory containing the application files (bin/, share/, etc.)
if [[ -d "$SCRIPT_DIR/bin" && -d "$SCRIPT_DIR/share" ]]; then
  SRC_DIR="$SCRIPT_DIR"
elif [[ -d "$SCRIPT_DIR/../../dist" ]]; then
  # Running from packaging/linux/ inside repository
  SRC_DIR=""
else
  SRC_DIR="$SCRIPT_DIR"
fi

PREFIX="${HOME}/.local"
DESTDIR=""
IS_UPDATE=0
FORCE=0
DRY_RUN=0
NON_INTERACTIVE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      shift 2
      ;;
    --destdir)
      DESTDIR="$2"
      shift 2
      ;;
    --update)
      IS_UPDATE=1
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --non-interactive|-y)
      NON_INTERACTIVE=1
      shift
      ;;
    -h|--help)
      sed -n '2,17p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      echo "Run '$0 --help' for usage." >&2
      exit 2
      ;;
  esac
done

# If SRC_DIR is not set (e.g. running from repo), find the staged or built files
if [[ -z "$SRC_DIR" ]]; then
  echo "error: install.sh must be run from an extracted CADCraft package." >&2
  exit 1
fi

# Ensure bin/cadcraft exists in SRC_DIR
if [[ ! -f "$SRC_DIR/bin/cadcraft" ]]; then
  echo "error: $SRC_DIR/bin/cadcraft not found. Invalid package directory." >&2
  exit 1
fi

# Validate prefix
if [[ -z "$PREFIX" || "$PREFIX" == "/" ]]; then
  echo "error: invalid installation prefix: '$PREFIX'" >&2
  exit 1
fi

# Prevent installing into the repository or package source directory
RESOLVED_PREFIX="$(mkdir -p "$PREFIX" 2>/dev/null && cd "$PREFIX" && pwd || echo "$PREFIX")"
if [[ "$RESOLVED_PREFIX" == "$SRC_DIR" ]]; then
  echo "error: installation prefix cannot be the package source directory ($SRC_DIR)." >&2
  exit 1
fi
if [[ -d "$SRC_DIR/.git" && "$RESOLVED_PREFIX" == "$(cd "$SRC_DIR" && pwd)" ]]; then
  echo "error: cannot install directly into the git repository." >&2
  exit 1
fi

TARGET_ROOT="${DESTDIR}${PREFIX}"
TARGET_BIN="${TARGET_ROOT}/bin"
TARGET_SHARE="${TARGET_ROOT}/share"
TARGET_APPS="${TARGET_SHARE}/applications"
TARGET_ICONS="${TARGET_SHARE}/icons"
TARGET_MIME="${TARGET_SHARE}/mime"
TARGET_METAINFO="${TARGET_SHARE}/metainfo"
TARGET_DOC="${TARGET_SHARE}/doc/cadcraft"
TARGET_CADCRAFT="${TARGET_SHARE}/cadcraft"

# Check if CADCraft is running from target installation
if [[ "$DRY_RUN" -eq 0 && "$FORCE" -eq 0 ]]; then
  RUNNING_PIDS=""
  if command -v pgrep >/dev/null 2>&1; then
    RUNNING_PIDS="$(pgrep -x cadcraft 2>/dev/null || true)"
  fi
  if [[ -n "$RUNNING_PIDS" ]]; then
    # Verify if any running pid corresponds to this target bin
    MATCH=0
    for pid in $RUNNING_PIDS; do
      if [[ -e "/proc/$pid/exe" ]]; then
        EXE_PATH="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
        if [[ "$EXE_PATH" == "${TARGET_BIN}/cadcraft" ]]; then
          MATCH=1
          break
        fi
      else
        MATCH=1
      fi
    done
    if [[ "$MATCH" -eq 1 ]]; then
      echo "error: CADCraft is currently running (PID: $RUNNING_PIDS)." >&2
      echo "Please save your drawings and close CADCraft before installing or updating." >&2
      echo "Use --force to override if you are certain." >&2
      exit 1
    fi
  fi
fi

# Check existing installation
EXISTING_MANIFEST="${TARGET_CADCRAFT}/install-manifest.txt"
PREVIOUS_MANAGED_FILES=()
if [[ -f "$EXISTING_MANIFEST" ]]; then
  IS_UPDATE=1
  while IFS= read -r line; do
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    PREVIOUS_MANAGED_FILES+=("$line")
  done < "$EXISTING_MANIFEST"
fi

# Read version metadata if available
PKG_VERSION="unknown"
PKG_SHA="unknown"
PKG_DATE="unknown"
if [[ -f "$SRC_DIR/version.env" ]]; then
  # shellcheck source=/dev/null
  . "$SRC_DIR/version.env"
  PKG_VERSION="${CADCRAFT_VERSION:-unknown}"
  PKG_SHA="${CADCRAFT_BUILD_SHA:-unknown}"
  PKG_DATE="${CADCRAFT_BUILD_DATE:-unknown}"
fi

echo "=================================================================="
if [[ "$IS_UPDATE" -eq 1 ]]; then
  echo " CADCraft — Updating Installation"
else
  echo " CADCraft — User Installation"
fi
echo "=================================================================="
echo "Version:      $PKG_VERSION (${PKG_SHA:0:10})"
echo "Source:       $SRC_DIR"
echo "Destination:  $PREFIX"
if [[ -n "$DESTDIR" ]]; then
  echo "DestDir root: $DESTDIR"
fi
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "Mode:         DRY RUN (no changes will be made)"
fi
echo "=================================================================="

# Collect list of candidate files that this package will install
CANDIDATE_FILES=("bin/cadcraft")
if [[ -f "$SRC_DIR/bin/cadcraft-cli" ]]; then
  CANDIDATE_FILES+=("bin/cadcraft-cli")
fi
if [[ -f "$SRC_DIR/share/applications/ai.storyteller.cadcraft.desktop" ]]; then
  CANDIDATE_FILES+=("share/applications/ai.storyteller.cadcraft.desktop")
fi
if [[ -d "$SRC_DIR/share/icons/hicolor" ]]; then
  while read -r icon_rel; do
    CANDIDATE_FILES+=("$icon_rel")
  done < <(cd "$SRC_DIR" && find share/icons/hicolor -type f)
fi
if [[ -f "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" ]]; then
  CANDIDATE_FILES+=("share/mime/packages/ai.storyteller.cadcraft.xml")
fi
if [[ -f "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" ]]; then
  CANDIDATE_FILES+=("share/metainfo/ai.storyteller.cadcraft.metainfo.xml")
fi
if [[ -d "$SRC_DIR/share/doc/cadcraft" ]]; then
  while read -r doc_rel; do
    CANDIDATE_FILES+=("$doc_rel")
  done < <(cd "$SRC_DIR" && find share/doc/cadcraft -type f)
fi
CANDIDATE_FILES+=("share/cadcraft/uninstall.sh")
CANDIDATE_FILES+=("share/cadcraft/version.env")
CANDIDATE_FILES+=("share/cadcraft/install-manifest.txt")

# Collision detection: verify if any destination file already exists and is NOT managed by previous CADCraft install
if [[ "$FORCE" -eq 0 ]]; then
  COLLISIONS=()
  for cand in "${CANDIDATE_FILES[@]}"; do
    target_path="${TARGET_ROOT}/${cand}"
    if [[ -e "$target_path" || -L "$target_path" ]]; then
      # Check if cand was in previous manifest
      WAS_MANAGED=0
      for prev in "${PREVIOUS_MANAGED_FILES[@]}"; do
        if [[ "$prev" == "$cand" ]]; then
          WAS_MANAGED=1
          break
        fi
      done
      if [[ "$WAS_MANAGED" -eq 0 ]]; then
        COLLISIONS+=("$cand")
      fi
    fi
  done
  if [[ ${#COLLISIONS[@]} -gt 0 ]]; then
    echo "error: collision detected with unmanaged existing file(s) in $TARGET_ROOT:" >&2
    for c in "${COLLISIONS[@]}"; do
      echo "  - $TARGET_ROOT/$c" >&2
    done
    echo "These files already exist and are not managed by CADCraft." >&2
    echo "Refusing to overwrite unrelated files. Use --force to override." >&2
    exit 1
  fi
fi

if [[ "$NON_INTERACTIVE" -eq 0 && "$DRY_RUN" -eq 0 ]]; then
  if [[ -t 0 ]]; then
    read -r -p "Proceed with installation? [Y/n] " CONFIRM
    if [[ "$CONFIRM" =~ ^[nN] ]]; then
      echo "Installation cancelled by user."
      exit 0
    fi
  fi
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[dry-run] Would create directories under $TARGET_ROOT"
  echo "[dry-run] Would copy executables to $TARGET_BIN"
  echo "[dry-run] Would configure .desktop file with Exec=\"$PREFIX/bin/cadcraft\" %F"
  echo "[dry-run] Would copy icons, mime, metainfo, and documentation"
  echo "[dry-run] Would write manifest to $TARGET_CADCRAFT/install-manifest.txt"
  echo "[dry-run] Would refresh desktop, MIME, and icon databases"
  exit 0
fi

# Temporary staging and backup directory for atomic installation/rollback
TMP_STAGE="$(mktemp -d "${TMPDIR:-/tmp}/cadcraft-install.XXXXXX")"
BACKUP_DIR="$TMP_STAGE/backup"
mkdir -p "$BACKUP_DIR"

INSTALL_SUCCESS=0
rollback_on_failure() {
  if [[ "$INSTALL_SUCCESS" -eq 0 ]]; then
    echo "" >&2
    echo "error: installation failed! Rolling back changes..." >&2
    # Restore any backed up files
    if [[ -d "$BACKUP_DIR" ]]; then
      (cd "$BACKUP_DIR" && find . -type f) | while read -r b_rel; do
        b_clean="${b_rel#./}"
        mkdir -p "$(dirname "$TARGET_ROOT/$b_clean")"
        cp -f "$BACKUP_DIR/$b_clean" "$TARGET_ROOT/$b_clean" 2>/dev/null || true
      done
    fi
    # Remove newly created files that were not in backup
    for new_f in "${INSTALLED_FILES[@]}"; do
      if [[ ! -f "$BACKUP_DIR/$new_f" && -f "$TARGET_ROOT/$new_f" ]]; then
        rm -f "$TARGET_ROOT/$new_f" 2>/dev/null || true
      fi
    done
    echo "Rollback complete. System returned to pre-installation state." >&2
  fi
  rm -rf "$TMP_STAGE"
}
trap rollback_on_failure EXIT

# Backup files that will be modified
for cand in "${CANDIDATE_FILES[@]}"; do
  target_file="$TARGET_ROOT/$cand"
  if [[ -f "$target_file" || -L "$target_file" ]]; then
    mkdir -p "$(dirname "$BACKUP_DIR/$cand")"
    cp -p "$target_file" "$BACKUP_DIR/$cand"
  fi
done

INSTALLED_FILES=()

# 1. Prepare target directories
mkdir -p "$TARGET_BIN"
mkdir -p "$TARGET_APPS"
mkdir -p "$TARGET_ICONS"
mkdir -p "$TARGET_MIME/packages"
mkdir -p "$TARGET_METAINFO"
mkdir -p "$TARGET_DOC"
mkdir -p "$TARGET_CADCRAFT"

# 2. Copy binaries
install -m 755 "$SRC_DIR/bin/cadcraft" "$TARGET_BIN/cadcraft"
INSTALLED_FILES+=("bin/cadcraft")

if [[ -f "$SRC_DIR/bin/cadcraft-cli" ]]; then
  install -m 755 "$SRC_DIR/bin/cadcraft-cli" "$TARGET_BIN/cadcraft-cli"
  INSTALLED_FILES+=("bin/cadcraft-cli")
fi

# 3. Configure and install .desktop launcher
# Must resolve the exact installed executable without relying on ~/.local/bin in GUI $PATH.
# Uses proper double-quoting for the Exec field per FreeDesktop Desktop Entry specification.
DESKTOP_SRC="$SRC_DIR/share/applications/ai.storyteller.cadcraft.desktop"
if [[ -f "$DESKTOP_SRC" ]]; then
  ESCAPED_PREFIX="${PREFIX//\"/\\\"}"
  sed \
    -e "s|^Exec=.*|Exec=\"${ESCAPED_PREFIX}/bin/cadcraft\" %F|" \
    -e "s|^TryExec=.*|TryExec=${ESCAPED_PREFIX}/bin/cadcraft|" \
    "$DESKTOP_SRC" > "$TMP_STAGE/ai.storyteller.cadcraft.desktop"

  install -m 644 "$TMP_STAGE/ai.storyteller.cadcraft.desktop" "$TARGET_APPS/ai.storyteller.cadcraft.desktop"
  INSTALLED_FILES+=("share/applications/ai.storyteller.cadcraft.desktop")
fi

# 4. Copy icons
if [[ -d "$SRC_DIR/share/icons/hicolor" ]]; then
  (cd "$SRC_DIR" && find share/icons/hicolor -type f) | while read -r icon_rel; do
    icon_dest="$TARGET_ROOT/$icon_rel"
    mkdir -p "$(dirname "$icon_dest")"
    install -m 644 "$SRC_DIR/$icon_rel" "$icon_dest"
  done
  while read -r icon_rel; do
    INSTALLED_FILES+=("$icon_rel")
  done < <(cd "$SRC_DIR" && find share/icons/hicolor -type f)
fi

# 5. Copy MIME definitions
if [[ -f "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" ]]; then
  install -m 644 "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" "$TARGET_MIME/packages/ai.storyteller.cadcraft.xml"
  INSTALLED_FILES+=("share/mime/packages/ai.storyteller.cadcraft.xml")
fi

# 6. Copy AppStream metadata
if [[ -f "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" ]]; then
  install -m 644 "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" "$TARGET_METAINFO/ai.storyteller.cadcraft.metainfo.xml"
  INSTALLED_FILES+=("share/metainfo/ai.storyteller.cadcraft.metainfo.xml")
fi

# 7. Copy documentation and licenses
if [[ -d "$SRC_DIR/share/doc/cadcraft" ]]; then
  (cd "$SRC_DIR" && find share/doc/cadcraft -type f) | while read -r doc_rel; do
    doc_dest="$TARGET_ROOT/$doc_rel"
    mkdir -p "$(dirname "$doc_dest")"
    install -m 644 "$SRC_DIR/$doc_rel" "$doc_dest"
  done
  while read -r doc_rel; do
    INSTALLED_FILES+=("$doc_rel")
  done < <(cd "$SRC_DIR" && find share/doc/cadcraft -type f)
fi

# 8. Install uninstaller and metadata
if [[ -f "$SRC_DIR/uninstall.sh" ]]; then
  install -m 755 "$SRC_DIR/uninstall.sh" "$TARGET_CADCRAFT/uninstall.sh"
  INSTALLED_FILES+=("share/cadcraft/uninstall.sh")
fi

# Record version info
cat > "$TARGET_CADCRAFT/version.env" <<EOF
CADCRAFT_VERSION="$PKG_VERSION"
CADCRAFT_BUILD_SHA="$PKG_SHA"
CADCRAFT_BUILD_DATE="$PKG_DATE"
CADCRAFT_INSTALL_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
CADCRAFT_PREFIX="$PREFIX"
EOF
INSTALLED_FILES+=("share/cadcraft/version.env")

# 9. Clean up obsolete files from previous version if updating
if [[ -f "$EXISTING_MANIFEST" ]]; then
  while IFS= read -r old_file; do
    [[ -z "$old_file" || "$old_file" =~ ^# ]] && continue
    # Strict validation of path
    if [[ "$old_file" == /* || "$old_file" == *".."* ]]; then
      continue
    fi
    # If the file was not part of the new installation, remove it safely
    STILL_EXISTS=0
    for new_file in "${INSTALLED_FILES[@]}"; do
      if [[ "$old_file" == "$new_file" ]]; then
        STILL_EXISTS=1
        break
      fi
    done
    if [[ "$STILL_EXISTS" -eq 0 && -f "$TARGET_ROOT/$old_file" ]]; then
      rm -f "$TARGET_ROOT/$old_file"
    fi
  done < "$EXISTING_MANIFEST"
fi

# 10. Write new manifest
MANIFEST_TEMP="$TMP_STAGE/manifest.txt"
{
  echo "# CADCraft installation manifest"
  echo "# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "# Prefix: $PREFIX"
  for f in "${INSTALLED_FILES[@]}"; do
    echo "$f"
  done
  echo "share/cadcraft/install-manifest.txt"
} > "$MANIFEST_TEMP"
install -m 644 "$MANIFEST_TEMP" "$TARGET_CADCRAFT/install-manifest.txt"

# 11. Refresh desktop, MIME and icon caches
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "$TARGET_APPS" 2>/dev/null || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database "$TARGET_MIME" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f "$TARGET_ICONS/hicolor" 2>/dev/null || true
fi

INSTALL_SUCCESS=1

echo ""
echo "Installation complete!"
echo "Files installed to: $PREFIX"
echo ""
echo "Usage:"
echo "  1. Desktop Menu: Search for 'CADCraft' in your Application Menu."
echo "  2. Terminal:     $PREFIX/bin/cadcraft [drawing.dwg|drawing.dxf]"
echo "  3. File Manager: Right-click any DWG or DXF file and select 'Open With -> CADCraft'."
echo ""

# Check if PREFIX/bin is in PATH
case ":$PATH:" in
  *":${PREFIX}/bin:"*) ;;
  *)
    echo "Note: '${PREFIX}/bin' is not in your current PATH."
    echo "To launch 'cadcraft' directly from any terminal, add this to your ~/.bashrc or ~/.profile:"
    echo "  export PATH=\"${PREFIX}/bin:\$PATH\""
    echo ""
    ;;
esac

echo "Uninstallation:"
echo "  To uninstall CADCraft completely (without affecting your drawings or settings):"
echo "  $PREFIX/share/cadcraft/uninstall.sh"
echo "=================================================================="
exit 0
