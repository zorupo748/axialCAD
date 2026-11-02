#!/usr/bin/env bash
# uninstall.sh — Cleanly uninstalls CADCraft from a user prefix.
# Preserves user settings (~/.config/cadcraft), session data, and user drawings.
#
# Usage:
#   ./uninstall.sh [OPTIONS]
#
# Options:
#   --prefix <path>     Installation prefix to remove from (default: auto-detected or $HOME/.local)
#   --destdir <path>    Staging root directory
#   --dry-run           Display what would be removed without deleting files
#   --non-interactive   Run without prompting for confirmation (-y)
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

# Expand leading tilde in PREFIX if present
PREFIX="${PREFIX/#\~/$HOME}"

if [[ -z "$PREFIX" || "$PREFIX" == "/" ]]; then
  echo "error: invalid or root prefix specified: '$PREFIX'" >&2
  exit 1
fi

TARGET_ROOT="${DESTDIR}${PREFIX}"
TARGET_CADCRAFT="${TARGET_ROOT}/share/cadcraft"
MANIFEST="${TARGET_CADCRAFT}/install-manifest.txt"
LOCK_FILE="${TARGET_CADCRAFT}/.install.lock"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: CADCraft installation manifest not found at: $MANIFEST" >&2
  echo "CADCraft does not appear to be installed in: $PREFIX" >&2
  exit 1
fi

# -----------------------------------------------------------------------------
# Locking and Parent Boundary Checks: Exige flock y valida límites
# -----------------------------------------------------------------------------
if ! command -v flock >/dev/null 2>&1; then
  echo "error: required system tool 'flock' is not available in PATH." >&2
  echo "flock is mandatory to prevent concurrent modifications and race conditions." >&2
  exit 1
fi

CANON_ROOT="$(mkdir -p "$TARGET_ROOT" 2>/dev/null && cd "$TARGET_ROOT" && pwd || echo "$TARGET_ROOT")"

verify_parent_boundaries() {
  local dir="$1"
  if [[ -L "$dir" ]]; then
    local canon_link
    canon_link="$(readlink -f "$dir" 2>/dev/null || true)"
    if [[ "$canon_link" != "$CANON_ROOT" && "$canon_link" != "$CANON_ROOT/"* ]]; then
      echo "error: managed directory '$dir' is an unexpected symlink pointing outside installation prefix: $canon_link" >&2
      exit 1
    fi
  fi
}

verify_parent_boundaries "${TARGET_ROOT}/bin"
verify_parent_boundaries "${TARGET_ROOT}/share"
verify_parent_boundaries "$TARGET_CADCRAFT"
verify_parent_boundaries "${TARGET_CADCRAFT}/versions"

LOCK_FD=200
LOCK_ACQUIRED=0

acquire_lock() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    return 0
  fi
  mkdir -p "$TARGET_CADCRAFT" 2>/dev/null || true
  eval "exec ${LOCK_FD}>\"\$LOCK_FILE\""
  if ! flock -n "$LOCK_FD"; then
    echo "error: another CADCraft operation is currently running in $TARGET_ROOT." >&2
    exit 1
  fi
  LOCK_ACQUIRED=1
}

release_lock() {
  if [[ "$LOCK_ACQUIRED" -eq 1 ]]; then
    flock -u "$LOCK_FD" 2>/dev/null || true
    eval "exec ${LOCK_FD}>&-" 2>/dev/null || true
    LOCK_ACQUIRED=0
  fi
}

trap release_lock EXIT
acquire_lock

# -----------------------------------------------------------------------------
# Check if CADCraft or CADCraft CLI is running from target
# -----------------------------------------------------------------------------
if [[ "$DRY_RUN" -eq 0 ]]; then
  if ! command -v pgrep >/dev/null 2>&1 || ! command -v readlink >/dev/null 2>&1; then
    echo "error: required system tools ('pgrep' and 'readlink') are not available for process protection." >&2
    exit 1
  fi

  PIDS_CADCRAFT="$(pgrep -x cadcraft 2>/dev/null || true)"
  PIDS_CLI="$(pgrep -x cadcraft-cli 2>/dev/null || true)"
  RUNNING_PIDS="$(echo "$PIDS_CADCRAFT $PIDS_CLI" | tr ' ' '\n' | grep -v '^$' | sort -u || true)"

  if [[ -n "$RUNNING_PIDS" ]]; then
    CANON_BIN_CADCRAFT=""
    if [[ -e "${TARGET_ROOT}/bin/cadcraft" ]]; then
      CANON_BIN_CADCRAFT="$(readlink -f "${TARGET_ROOT}/bin/cadcraft" 2>/dev/null || true)"
    fi
    CANON_BIN_CLI=""
    if [[ -e "${TARGET_ROOT}/bin/cadcraft-cli" ]]; then
      CANON_BIN_CLI="$(readlink -f "${TARGET_ROOT}/bin/cadcraft-cli" 2>/dev/null || true)"
    fi
    CANON_CADCRAFT_SHARE=""
    if [[ -d "$TARGET_CADCRAFT" ]]; then
      CANON_CADCRAFT_SHARE="$(readlink -f "$TARGET_CADCRAFT" 2>/dev/null || true)"
    fi

    MATCHED_PIDS=()
    for pid in $RUNNING_PIDS; do
      if [[ -e "/proc/$pid/exe" ]]; then
        EXE_PATH="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
        IS_MATCH=0
        if [[ -n "$CANON_BIN_CADCRAFT" && "$EXE_PATH" == "$CANON_BIN_CADCRAFT" ]]; then
          IS_MATCH=1
        elif [[ -n "$CANON_BIN_CLI" && "$EXE_PATH" == "$CANON_BIN_CLI" ]]; then
          IS_MATCH=1
        elif [[ -n "$CANON_CADCRAFT_SHARE" && "$EXE_PATH" == "${CANON_CADCRAFT_SHARE}/"* ]]; then
          IS_MATCH=1
        elif [[ "$EXE_PATH" == "${TARGET_ROOT}/"* && ( "$EXE_PATH" == */cadcraft || "$EXE_PATH" == */cadcraft-cli ) ]]; then
          IS_MATCH=1
        fi
        if [[ "$IS_MATCH" -eq 1 ]]; then
          MATCHED_PIDS+=("$pid ($EXE_PATH)")
        fi
      fi
    done

    if [[ ${#MATCHED_PIDS[@]} -gt 0 ]]; then
      echo "error: CADCraft processes are currently running from target installation:" >&2
      for m in "${MATCHED_PIDS[@]}"; do
        echo "  - PID $m" >&2
      done
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
  read -r -p "Are you sure you want to uninstall CADCraft from $PREFIX? [y/N] " CONFIRM || CONFIRM="n"
  if [[ ! "$CONFIRM" =~ ^[yY] ]]; then
    echo "Uninstallation cancelled."
    exit 0
  fi
fi

REMOVED_COUNT=0

# Read manifest and remove files with strict path validation
while IFS= read -r rel_file; do
  [[ -z "$rel_file" || "$rel_file" =~ ^# ]] && continue

  if [[ "$rel_file" == /* ]]; then
    echo "warning: ignoring malicious/invalid absolute path in manifest: $rel_file" >&2
    continue
  fi

  if [[ "$rel_file" == *".."* ]]; then
    echo "warning: ignoring malicious/invalid traversal path in manifest: $rel_file" >&2
    continue
  fi

  file_path="${TARGET_ROOT}/${rel_file}"

  file_dir="$(dirname "$file_path")"
  canon_dir="$(cd "$file_dir" 2>/dev/null && pwd || echo "$file_dir")"
  if [[ "$canon_dir" != "$CANON_ROOT" && "$canon_dir" != "$CANON_ROOT/"* ]]; then
    echo "warning: path escapes installation root, skipping: $rel_file" >&2
    continue
  fi

  if [[ -e "$file_path" || -L "$file_path" ]]; then
    canon_file="$(readlink -f "$file_path" 2>/dev/null || true)"
    if [[ -n "$canon_file" && "$canon_file" != "$CANON_ROOT" && "$canon_file" != "$CANON_ROOT/"* ]]; then
      echo "warning: file resolves outside installation root, skipping: $rel_file" >&2
      continue
    fi
    if [[ "$DRY_RUN" -eq 1 ]]; then
      echo "[dry-run] Would remove: $file_path"
    else
      rm -f "$file_path"
      REMOVED_COUNT=$((REMOVED_COUNT + 1))
    fi
  fi
done < "$MANIFEST"

if [[ "$DRY_RUN" -eq 0 ]]; then
  rm -f "${TARGET_CADCRAFT}/current" 2>/dev/null || true
  rm -f "${TARGET_CADCRAFT}/previous" 2>/dev/null || true
  rm -f "${TARGET_CADCRAFT}/install.sh" 2>/dev/null || true
  rm -f "${TARGET_CADCRAFT}/uninstall.sh" 2>/dev/null || true
  rm -f "${TARGET_CADCRAFT}/version.env" 2>/dev/null || true
  rm -f "${TARGET_CADCRAFT}/install-manifest.txt" 2>/dev/null || true

  # Prune only completely empty directories; preserve any unmanaged foreign files
  if [[ -d "${TARGET_CADCRAFT}/versions" ]]; then
    find "${TARGET_CADCRAFT}/versions" -depth -type d -empty -delete 2>/dev/null || true
    rmdir "${TARGET_CADCRAFT}/versions" 2>/dev/null || true
  fi

  # Release the lock without unlinking the lockfile to prevent inode races with concurrent processes
  release_lock

  rmdir "${TARGET_CADCRAFT}" 2>/dev/null || true
  rmdir "${TARGET_ROOT}/share/doc/cadcraft" 2>/dev/null || true

  if [[ -d "${TARGET_ROOT}/share/icons/hicolor" ]]; then
    find "${TARGET_ROOT}/share/icons/hicolor" -type d -empty -delete 2>/dev/null || true
  fi

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
  echo "  Your personal settings (~/.config/cadcraft), session data, and drawings were NOT removed."
  echo "=================================================================="
else
  echo ""
  echo "[dry-run] Uninstallation dry-run complete."
fi

exit 0
