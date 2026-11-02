#!/usr/bin/env bash
# uninstall.sh — Cleanly uninstalls CADCraft from a user prefix.
# Preserves user settings (~/.config/cadcraft) and user drawings.
#
# Usage:
#   ./uninstall.sh [OPTIONS]
#
# Options:
#   --prefix <path>     Installation prefix to remove from (default: auto-detected or $HOME/.local)
#   --destdir <path>    Staging root directory
#   --dry-run           Display what would be removed without deleting files
#   --non-interactive   Run without prompting for confirmation
#   -h, --help          Show this help message
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Auto-detect PREFIX if run from <prefix>/share/cadcraft/uninstall.sh
if [[ "$SCRIPT_DIR" == */share/cadcraft ]]; then
  AUTO_PREFIX="$(cd "$SCRIPT_DIR/../.." && pwd)"
else
  AUTO_PREFIX="${HOME}/.local"
fi

PREFIX="$AUTO_PREFIX"
DESTDIR=""
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
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --non-interactive|-y)
      NON_INTERACTIVE=1
      shift
      ;;
    -h|--help)
      sed -n '2,14p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      echo "Run '$0 --help' for usage." >&2
      exit 2
      ;;
  esac
done

if [[ -z "$PREFIX" || "$PREFIX" == "/" ]]; then
  echo "error: invalid or root prefix specified: '$PREFIX'" >&2
  exit 1
fi

TARGET_ROOT="${DESTDIR}${PREFIX}"
MANIFEST="${TARGET_ROOT}/share/cadcraft/install-manifest.txt"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: CADCraft installation manifest not found at: $MANIFEST" >&2
  echo "CADCraft does not appear to be installed in: $PREFIX" >&2
  exit 1
fi

# Check if CADCraft is running from target
if [[ "$DRY_RUN" -eq 0 ]]; then
  RUNNING_PIDS=""
  if command -v pgrep >/dev/null 2>&1; then
    RUNNING_PIDS="$(pgrep -x cadcraft 2>/dev/null || true)"
  fi
  if [[ -n "$RUNNING_PIDS" ]]; then
    MATCH=0
    for pid in $RUNNING_PIDS; do
      if [[ -e "/proc/$pid/exe" ]]; then
        EXE_PATH="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
        if [[ "$EXE_PATH" == "${TARGET_ROOT}/bin/cadcraft" ]]; then
          MATCH=1
          break
        fi
      else
        MATCH=1
      fi
    done
    if [[ "$MATCH" -eq 1 ]]; then
      echo "error: CADCraft is currently running (PID: $RUNNING_PIDS)." >&2
      echo "Please close CADCraft before uninstalling." >&2
      exit 1
    fi
  fi
fi

echo "=================================================================="
echo " CADCraft — Uninstallation"
echo "=================================================================="
echo "Target Prefix: $PREFIX"
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "Mode:          DRY RUN (no files will be deleted)"
fi
echo "=================================================================="

if [[ "$NON_INTERACTIVE" -eq 0 && "$DRY_RUN" -eq 0 ]]; then
  if [[ -t 0 ]]; then
    read -r -p "Are you sure you want to uninstall CADCraft from $PREFIX? [y/N] " CONFIRM
    if [[ ! "$CONFIRM" =~ ^[yY] ]]; then
      echo "Uninstallation cancelled."
      exit 0
    fi
  fi
fi

REMOVED_COUNT=0

# Canonical prefix for bounds-checking
CANON_ROOT="$(mkdir -p "$TARGET_ROOT" 2>/dev/null && cd "$TARGET_ROOT" && pwd || echo "$TARGET_ROOT")"

# Read manifest and remove files with strict path validation
while IFS= read -r rel_file; do
  [[ -z "$rel_file" || "$rel_file" =~ ^# ]] && continue

  # 1. Reject absolute paths
  if [[ "$rel_file" == /* ]]; then
    echo "warning: ignoring malicious/invalid absolute path in manifest: $rel_file" >&2
    continue
  fi

  # 2. Reject paths containing parent traversal
  if [[ "$rel_file" == *".."* ]]; then
    echo "warning: ignoring malicious/invalid traversal path in manifest: $rel_file" >&2
    continue
  fi

  file_path="${TARGET_ROOT}/${rel_file}"

  # 3. Verify path stays within TARGET_ROOT
  file_dir="$(dirname "$file_path")"
  canon_dir="$(cd "$file_dir" 2>/dev/null && pwd || echo "$file_dir")"
  if [[ "$canon_dir" != "$CANON_ROOT"* ]]; then
    echo "warning: path escapes installation root, skipping: $rel_file" >&2
    continue
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    if [[ -e "$file_path" || -L "$file_path" ]]; then
      echo "[dry-run] Would remove: $file_path"
    fi
  else
    if [[ -e "$file_path" || -L "$file_path" ]]; then
      rm -f "$file_path"
      REMOVED_COUNT=$((REMOVED_COUNT + 1))
    fi
  fi
done < "$MANIFEST"

if [[ "$DRY_RUN" -eq 0 ]]; then
  # Remove CADCraft specific directories if empty
  rmdir "${TARGET_ROOT}/share/cadcraft" 2>/dev/null || true
  rmdir "${TARGET_ROOT}/share/doc/cadcraft" 2>/dev/null || true

  # Prune empty icon directories under hicolor without deleting parent folders
  if [[ -d "${TARGET_ROOT}/share/icons/hicolor" ]]; then
    find "${TARGET_ROOT}/share/icons/hicolor" -type d -empty -delete 2>/dev/null || true
  fi

  # Refresh desktop and MIME databases
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "${TARGET_ROOT}/share/applications" 2>/dev/null || true
  fi
  if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "${TARGET_ROOT}/share/mime" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "${TARGET_ROOT}/share/icons/hicolor" 2>/dev/null || true
  fi

  echo ""
  echo "CADCraft has been uninstalled successfully."
  echo "Total files removed: $REMOVED_COUNT"
  echo ""
  echo "Notice:"
  echo "  Your personal settings (~/.config/cadcraft) and user drawings were NOT removed."
  echo "=================================================================="
else
  echo ""
  echo "[dry-run] Uninstallation dry-run complete."
fi

exit 0
