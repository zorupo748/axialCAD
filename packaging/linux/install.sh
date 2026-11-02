#!/usr/bin/env bash
# install.sh — Installs, updates, or rolls back CADCraft for the current Linux user.
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
#   --rollback          Roll back to the previous version
#   --force             Override unmanaged file collisions in prefix (does NOT bypass running processes, containment, or immutable build identity)
#   --dry-run           Display actions without modifying files
#   --non-interactive   Run without interactive confirmation prompts (-y)
#   -h, --help          Show this help message
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source directory containing the application files (bin/, share/, etc.)
if [[ -d "$SCRIPT_DIR/bin" && -d "$SCRIPT_DIR/share" ]]; then
  SRC_DIR="$SCRIPT_DIR"
elif [[ "$SCRIPT_DIR" == */share/cadcraft ]]; then
  SRC_DIR=""
else
  SRC_DIR="$SCRIPT_DIR"
fi

PREFIX="${HOME}/.local"
DESTDIR=""
IS_UPDATE=0
IS_ROLLBACK=0
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
    --rollback)
      IS_ROLLBACK=1
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

if [[ "$IS_UPDATE" -eq 1 && "$IS_ROLLBACK" -eq 1 ]]; then
  echo "error: cannot specify both --update and --rollback." >&2
  exit 2
fi

# Expand leading tilde in PREFIX if present
PREFIX="${PREFIX/#\~/$HOME}"

# Validate prefix
if [[ -z "$PREFIX" || "$PREFIX" == "/" ]]; then
  echo "error: invalid installation prefix: '$PREFIX'" >&2
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
TARGET_VERSIONS="${TARGET_CADCRAFT}/versions"
LOCK_FILE="${TARGET_CADCRAFT}/.install.lock"

# Prevent installing into package source directory
if [[ -n "$SRC_DIR" ]]; then
  RESOLVED_PREFIX="$(mkdir -p "$PREFIX" 2>/dev/null && cd "$PREFIX" && pwd || echo "$PREFIX")"
  if [[ "$RESOLVED_PREFIX" == "$SRC_DIR" ]]; then
    echo "error: installation prefix cannot be the package source directory ($SRC_DIR)." >&2
    exit 1
  fi
  if [[ -d "$SRC_DIR/.git" && "$RESOLVED_PREFIX" == "$(cd "$SRC_DIR" && pwd)" ]]; then
    echo "error: cannot install directly into the git repository." >&2
    exit 1
  fi
fi

# -----------------------------------------------------------------------------
# System Protection Tools Check: Exige herramientas reales para integridad y procesos
# -----------------------------------------------------------------------------
if ! command -v flock >/dev/null 2>&1; then
  echo "error: required system tool 'flock' is not available in PATH." >&2
  echo "flock is mandatory to prevent concurrent modifications and race conditions." >&2
  exit 1
fi

if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
  echo "error: required SHA-256 utility not found (neither 'sha256sum' nor 'shasum' is in PATH)." >&2
  echo "A verified SHA-256 tool is required for security and integrity validation." >&2
  exit 1
fi

if ! command -v pgrep >/dev/null 2>&1 || ! command -v readlink >/dev/null 2>&1; then
  echo "error: required system tools ('pgrep' and 'readlink') are not available for process protection." >&2
  exit 1
fi

# -----------------------------------------------------------------------------
# Path and Parent Directory Verification: Reject unexpected symlinks in parents
# -----------------------------------------------------------------------------
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

verify_parent_boundaries "$TARGET_BIN"
verify_parent_boundaries "$TARGET_SHARE"
verify_parent_boundaries "$TARGET_CADCRAFT"
verify_parent_boundaries "$TARGET_VERSIONS"

# -----------------------------------------------------------------------------
# Locking: Prevent concurrent install / update / rollback operations
# -----------------------------------------------------------------------------
LOCK_FD=200
LOCK_ACQUIRED=0

acquire_lock() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    return 0
  fi
  mkdir -p "$TARGET_CADCRAFT"
  eval "exec ${LOCK_FD}>\"\$LOCK_FILE\""
  if ! flock -n "$LOCK_FD"; then
    echo "error: another CADCraft installation or rollback operation is currently running in $TARGET_ROOT." >&2
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

# -----------------------------------------------------------------------------
# Process inspection: check both cadcraft and cadcraft-cli (NO --force bypass)
# -----------------------------------------------------------------------------
check_cadcraft_running() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    return 0
  fi

  local pids_cadcraft=""
  local pids_cli=""
  pids_cadcraft="$(pgrep -x cadcraft 2>/dev/null || true)"
  pids_cli="$(pgrep -x cadcraft-cli 2>/dev/null || true)"
  local running_pids=""
  running_pids="$(echo "$pids_cadcraft $pids_cli" | tr ' ' '\n' | grep -v '^$' | sort -u || true)"

  [[ -z "$running_pids" ]] && return 0

  local canon_target_bin=""
  if [[ -e "${TARGET_BIN}/cadcraft" ]]; then
    canon_target_bin="$(readlink -f "${TARGET_BIN}/cadcraft" 2>/dev/null || true)"
  fi
  local canon_target_cli=""
  if [[ -e "${TARGET_BIN}/cadcraft-cli" ]]; then
    canon_target_cli="$(readlink -f "${TARGET_BIN}/cadcraft-cli" 2>/dev/null || true)"
  fi

  local canon_cadcraft_share=""
  if [[ -d "$TARGET_CADCRAFT" ]]; then
    canon_cadcraft_share="$(readlink -f "$TARGET_CADCRAFT" 2>/dev/null || true)"
  fi

  local matched_pids=()
  for pid in $running_pids; do
    if [[ -e "/proc/$pid/exe" ]]; then
      local exe_path
      exe_path="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
      local is_match=0

      if [[ -n "$canon_target_bin" && "$exe_path" == "$canon_target_bin" ]]; then
        is_match=1
      elif [[ -n "$canon_target_cli" && "$exe_path" == "$canon_target_cli" ]]; then
        is_match=1
      elif [[ -n "$canon_cadcraft_share" && "$exe_path" == "${canon_cadcraft_share}/"* ]]; then
        is_match=1
      elif [[ "$exe_path" == "${TARGET_ROOT}/"* && ( "$exe_path" == */cadcraft || "$exe_path" == */cadcraft-cli ) ]]; then
        is_match=1
      fi

      if [[ "$is_match" -eq 1 ]]; then
        matched_pids+=("$pid ($exe_path)")
      fi
    fi
  done

  if [[ ${#matched_pids[@]} -gt 0 ]]; then
    echo "error: CADCraft processes are currently running from target installation:" >&2
    for m in "${matched_pids[@]}"; do
      echo "  - PID $m" >&2
    done
    echo "Please save your drawings and close CADCraft before installing, updating, or rolling back." >&2
    echo "For safety and data integrity, process bypass is not permitted." >&2
    exit 1
  fi
}

# -----------------------------------------------------------------------------
# Metadata Parser: Strict validation for packages, explicit legacy migration parser
# -----------------------------------------------------------------------------
PARSED_VERSION=""
PARSED_SHA=""
PARSED_DATE=""
PARSED_ARCH=""

parse_package_version_env() {
  local env_file="$1"

  PARSED_VERSION=""
  PARSED_SHA=""
  PARSED_DATE=""
  PARSED_ARCH=""

  if [[ ! -f "$env_file" ]]; then
    echo "error: metadata file not found: $env_file" >&2
    exit 1
  fi

  local seen_version=0
  local seen_sha=0
  local seen_date=0
  local seen_arch=0

  while IFS='=' read -r key val || [[ -n "$key" ]]; do
    key="$(echo "$key" | tr -d '[:space:]')"
    [[ -z "$key" || "$key" =~ ^# ]] && continue
    if [[ ! "$key" =~ ^[A-Za-z0-9_]+$ ]]; then
      echo "error: invalid metadata key '$key' in $env_file" >&2
      exit 1
    fi

    val="${val#\"}"
    val="${val%\"}"
    val="${val#\'}"
    val="${val%\'}"

    case "$key" in
      CADCRAFT_VERSION)
        if [[ "$seen_version" -eq 1 ]]; then
          echo "error: duplicate CADCRAFT_VERSION key in $env_file" >&2
          exit 1
        fi
        seen_version=1
        if [[ ! "$val" =~ ^[0-9]+(\.[0-9]+)+(-[A-Za-z0-9._+-]+)?$ ]]; then
          echo "error: invalid CADCRAFT_VERSION format in $env_file: '$val'" >&2
          exit 1
        fi
        PARSED_VERSION="$val"
        ;;
      CADCRAFT_BUILD_SHA)
        if [[ "$seen_sha" -eq 1 ]]; then
          echo "error: duplicate CADCRAFT_BUILD_SHA key in $env_file" >&2
          exit 1
        fi
        seen_sha=1
        if [[ ! "$val" =~ ^[0-9a-fA-F]{40}$ ]]; then
          echo "error: CADCRAFT_BUILD_SHA in $env_file must be a 40-character hexadecimal SHA (got '$val')" >&2
          exit 1
        fi
        PARSED_SHA="$val"
        ;;
      CADCRAFT_BUILD_DATE)
        if [[ "$seen_date" -eq 1 ]]; then
          echo "error: duplicate CADCRAFT_BUILD_DATE key in $env_file" >&2
          exit 1
        fi
        seen_date=1
        if [[ ! "$val" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}:[0-9]{2}Z?)?$ ]]; then
          echo "error: invalid CADCRAFT_BUILD_DATE format in $env_file: '$val'" >&2
          exit 1
        fi
        PARSED_DATE="$val"
        ;;
      CADCRAFT_ARCH)
        if [[ "$seen_arch" -eq 1 ]]; then
          echo "error: duplicate CADCRAFT_ARCH key in $env_file" >&2
          exit 1
        fi
        seen_arch=1
        if [[ ! "$val" =~ ^[A-Za-z0-9_-]+$ ]]; then
          echo "error: invalid CADCRAFT_ARCH in $env_file: '$val'" >&2
          exit 1
        fi
        PARSED_ARCH="$val"
        ;;
      *)
        echo "error: unrecognized metadata key '$key' in $env_file" >&2
        exit 1
        ;;
    esac
  done < "$env_file"

  # All 4 metadata keys are mandatory in new packages
  if [[ "$seen_version" -eq 0 || -z "$PARSED_VERSION" ]]; then
    echo "error: missing required metadata 'CADCRAFT_VERSION' in $env_file" >&2
    exit 1
  fi
  if [[ "$seen_sha" -eq 0 || -z "$PARSED_SHA" ]]; then
    echo "error: missing required metadata 'CADCRAFT_BUILD_SHA' in $env_file" >&2
    exit 1
  fi
  if [[ "$seen_date" -eq 0 || -z "$PARSED_DATE" ]]; then
    echo "error: missing required metadata 'CADCRAFT_BUILD_DATE' in $env_file" >&2
    exit 1
  fi
  if [[ "$seen_arch" -eq 0 || -z "$PARSED_ARCH" ]]; then
    echo "error: missing required metadata 'CADCRAFT_ARCH' in $env_file" >&2
    exit 1
  fi
}

# Explicit parser for legacy pre-versioned metadata only
parse_legacy_version_env() {
  local env_file="$1"
  PARSED_VERSION="legacy"
  PARSED_SHA="unknown"
  PARSED_DATE=""
  PARSED_ARCH=""

  [[ -f "$env_file" ]] || return 0

  while IFS='=' read -r key val || [[ -n "$key" ]]; do
    key="$(echo "$key" | tr -d '[:space:]')"
    [[ -z "$key" || "$key" =~ ^# ]] && continue
    val="${val#\"}"
    val="${val%\"}"
    val="${val#\'}"
    val="${val%\'}"
    case "$key" in
      CADCRAFT_VERSION) [[ -n "$val" ]] && PARSED_VERSION="$val" ;;
      CADCRAFT_BUILD_SHA) [[ -n "$val" ]] && PARSED_SHA="$val" ;;
      CADCRAFT_BUILD_DATE) PARSED_DATE="$val" ;;
      CADCRAFT_ARCH) PARSED_ARCH="$val" ;;
    esac
  done < "$env_file"
}

# Real SHA-256 tool requirement (no fallback to size or mtime)
compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | cut -d' ' -f1
  else
    echo "error: SHA-256 utility not found (neither 'sha256sum' nor 'shasum' is available in PATH)." >&2
    echo "A verified SHA-256 tool is required for security and integrity validation." >&2
    exit 1
  fi
}

# Run command with timeout
run_with_timeout() {
  local sec="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout "$sec" "$@"
  else
    "$@" &
    local cmd_pid=$!
    ( sleep "$sec" && kill -9 "$cmd_pid" 2>/dev/null ) &
    local watcher_pid=$!
    local ret=0
    wait "$cmd_pid" 2>/dev/null || ret=$?
    kill "$watcher_pid" 2>/dev/null || true
    wait "$watcher_pid" 2>/dev/null || true
    return "$ret"
  fi
}

# Refresh desktop, MIME, and icon databases
refresh_desktop_databases() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$TARGET_APPS" 2>/dev/null || true
  fi
  if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$TARGET_MIME" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$TARGET_ICONS/hicolor" 2>/dev/null || true
  fi
}

# -----------------------------------------------------------------------------
# Transaction Recovery Manager: Captures state before mutations
# -----------------------------------------------------------------------------
RECOVERY_DIR="${TARGET_CADCRAFT}/.recovery.$$"
TRANSACTION_COMMITTED=0
NEW_BUILD_DIR_CREATED=0
STAGE_DIR=""
TARGET_BUILD_DIR=""
MIGRATED_LEGACY_DIR=""

record_created_file() {
  local rel="$1"
  [[ -d "$RECOVERY_DIR" ]] || return 0
  # If it existed before this operation, do not record it as created
  if [[ -f "$RECOVERY_DIR/existed_files.txt" ]] && grep -Fxq "$rel" "$RECOVERY_DIR/existed_files.txt" 2>/dev/null; then
    return 0
  fi
  # Record it as created by this operation so recovery removes it
  if [[ ! -f "$RECOVERY_DIR/created_files.txt" ]] || ! grep -Fxq "$rel" "$RECOVERY_DIR/created_files.txt" 2>/dev/null; then
    echo "$rel" >> "$RECOVERY_DIR/created_files.txt"
  fi
}

recover_transaction() {
  local orig_exit_code=$?
  if [[ "$TRANSACTION_COMMITTED" -eq 1 ]]; then
    rm -rf "$RECOVERY_DIR" 2>/dev/null || true
    [[ -n "$STAGE_DIR" ]] && rm -rf "$STAGE_DIR" 2>/dev/null || true
    release_lock
    return 0
  fi

  echo "" >&2
  echo "error: operation did not complete successfully (exit code $orig_exit_code). Initiating rollback recovery..." >&2

  local recovery_failed=0

  # Clean up staging directory
  [[ -n "$STAGE_DIR" ]] && rm -rf "$STAGE_DIR" 2>/dev/null || true

  # If a new build directory was created in this run, remove it
  if [[ "$NEW_BUILD_DIR_CREATED" -eq 1 && -n "$TARGET_BUILD_DIR" && -d "$TARGET_BUILD_DIR" ]]; then
    if ! rm -rf "$TARGET_BUILD_DIR"; then
      recovery_failed=1
    fi
  fi

  # If a legacy migration directory was created in this run, remove it to leave flat install intact
  if [[ -n "$MIGRATED_LEGACY_DIR" && -d "$MIGRATED_LEGACY_DIR" ]]; then
    if ! rm -rf "$MIGRATED_LEGACY_DIR"; then
      recovery_failed=1
    fi
  fi

  # If versions directory is empty, remove it so flat installs don't retain an empty versions/ folder
  if [[ -d "${TARGET_CADCRAFT}/versions" ]]; then
    rmdir "${TARGET_CADCRAFT}/versions" 2>/dev/null || true
  fi

  # Remove any newly created outside files that did not exist before operation
  if [[ -f "$RECOVERY_DIR/created_files.txt" ]]; then
    while IFS= read -r c_file || [[ -n "$c_file" ]]; do
      [[ -z "$c_file" ]] && continue
      if [[ -e "$TARGET_ROOT/$c_file" || -L "$TARGET_ROOT/$c_file" ]]; then
        if ! rm -f "$TARGET_ROOT/$c_file"; then
          recovery_failed=1
        fi
      fi
    done < "$RECOVERY_DIR/created_files.txt"
  fi

  # Restore current symlink
  if [[ -f "$RECOVERY_DIR/current.target" ]]; then
    local orig_curr
    orig_curr="$(cat "$RECOVERY_DIR/current.target")"
    if [[ -n "$orig_curr" ]]; then
      local curr_tmp="${TARGET_CADCRAFT}/current.tmp.$$"
      if ! ln -s "$orig_curr" "$curr_tmp" || ! mv -Tf "$curr_tmp" "${TARGET_CADCRAFT}/current"; then
        rm -f "$curr_tmp" 2>/dev/null || true
        recovery_failed=1
      fi
    else
      if [[ -e "${TARGET_CADCRAFT}/current" || -L "${TARGET_CADCRAFT}/current" ]]; then
        if ! rm -f "${TARGET_CADCRAFT}/current"; then
          recovery_failed=1
        fi
      fi
    fi
  fi

  # Restore previous symlink
  if [[ -f "$RECOVERY_DIR/previous.target" ]]; then
    local orig_prev
    orig_prev="$(cat "$RECOVERY_DIR/previous.target")"
    if [[ -n "$orig_prev" ]]; then
      local prev_tmp="${TARGET_CADCRAFT}/previous.tmp.$$"
      if ! ln -s "$orig_prev" "$prev_tmp" || ! mv -Tf "$prev_tmp" "${TARGET_CADCRAFT}/previous"; then
        rm -f "$prev_tmp" 2>/dev/null || true
        recovery_failed=1
      fi
    else
      if [[ -e "${TARGET_CADCRAFT}/previous" || -L "${TARGET_CADCRAFT}/previous" ]]; then
        if ! rm -f "${TARGET_CADCRAFT}/previous"; then
          recovery_failed=1
        fi
      fi
    fi
  fi

  # Restore entry binaries in TARGET_BIN safely distinguishing absent, symlink, and regular
  restore_entry_binary() {
    local name="$1"
    local bin_dest="${TARGET_BIN}/${name}"
    local type_file="$RECOVERY_DIR/bin_${name}.type"

    [[ -f "$type_file" ]] || return 0
    local bin_type
    bin_type="$(cat "$type_file")"

    case "$bin_type" in
      absent)
        if [[ -e "$bin_dest" || -L "$bin_dest" ]]; then
          if ! rm -f "$bin_dest"; then
            recovery_failed=1
          fi
        fi
        ;;
      symlink)
        local target
        target="$(cat "$RECOVERY_DIR/bin_${name}.val" 2>/dev/null || true)"
        if [[ -n "$target" ]]; then
          local tmp_link="${TARGET_BIN}/${name}.tmp.$$"
          if ! ln -s "$target" "$tmp_link" || ! mv -Tf "$tmp_link" "$bin_dest"; then
            rm -f "$tmp_link" 2>/dev/null || true
            recovery_failed=1
          fi
        else
          if [[ -e "$bin_dest" || -L "$bin_dest" ]]; then
            if ! rm -f "$bin_dest"; then
              recovery_failed=1
            fi
          fi
        fi
        ;;
      regular)
        local backup_file="$RECOVERY_DIR/bin_${name}.file"
        local tmp_file="${TARGET_BIN}/${name}.tmp.$$"
        # Copy to adjacent temporary file preserving original content and permissions, then atomic replace
        if [[ "${CADCRAFT_TEST_MOCK_CP_FAIL:-0}" -eq 1 ]] || ! cp -p "$backup_file" "$tmp_file" || ! mv -Tf "$tmp_file" "$bin_dest"; then
          rm -f "$tmp_file" 2>/dev/null || true
          recovery_failed=1
        fi
        ;;
    esac
  }

  restore_entry_binary "cadcraft"
  restore_entry_binary "cadcraft-cli"

  # Restore backed up shared files without pipeline subshell and propagating errors
  if [[ -d "$RECOVERY_DIR/files" ]]; then
    while IFS= read -r f_rel || [[ -n "$f_rel" ]]; do
      [[ -z "$f_rel" ]] && continue
      f_clean="${f_rel#./}"
      local dest_dir
      dest_dir="$(dirname "$TARGET_ROOT/$f_clean")"
      if ! mkdir -p "$dest_dir"; then
        recovery_failed=1
        continue
      fi
      if [[ "${CADCRAFT_TEST_MOCK_CP_FAIL:-0}" -eq 1 ]] || ! cp -p "$RECOVERY_DIR/files/$f_clean" "$TARGET_ROOT/$f_clean"; then
        recovery_failed=1
      fi
    done < <(cd "$RECOVERY_DIR/files" && find . -type f)
  fi

  # Prune empty directories created during failed operation
  if [[ -d "$TARGET_DOC" ]]; then
    rmdir "$TARGET_DOC" 2>/dev/null || true
  fi
  if [[ -d "${TARGET_ROOT}/share/icons/hicolor" ]]; then
    find "${TARGET_ROOT}/share/icons/hicolor" -depth -type d -empty -delete 2>/dev/null || true
  fi
  if [[ -d "${TARGET_CADCRAFT}/versions" ]]; then
    rmdir "${TARGET_CADCRAFT}/versions" 2>/dev/null || true
  fi
  if [[ -d "$TARGET_CADCRAFT" ]]; then
    rmdir "$TARGET_CADCRAFT" 2>/dev/null || true
  fi

  if [[ "$recovery_failed" -eq 0 ]]; then
    echo "Recovery completed: system restored cleanly to pre-operation state." >&2
    rm -rf "$RECOVERY_DIR" 2>/dev/null || true
  else
    echo "FATAL: recovery encountered errors! Backup state preserved in: $RECOVERY_DIR" >&2
    echo "Warning: target installation may be in an incomplete recovery state." >&2
  fi

  release_lock

  if [[ "$recovery_failed" -ne 0 ]]; then
    exit 1
  elif [[ "$orig_exit_code" -ne 0 ]]; then
    exit "$orig_exit_code"
  fi
}
trap recover_transaction EXIT

snapshot_state_for_recovery() {
  mkdir -p "$RECOVERY_DIR/files"
  touch "$RECOVERY_DIR/existed_files.txt"
  touch "$RECOVERY_DIR/created_files.txt"

  # Record current/previous symlink targets
  if [[ -L "${TARGET_CADCRAFT}/current" ]]; then
    readlink "${TARGET_CADCRAFT}/current" > "$RECOVERY_DIR/current.target"
    echo "share/cadcraft/current" >> "$RECOVERY_DIR/existed_files.txt"
  else
    touch "$RECOVERY_DIR/current.target"
  fi

  if [[ -L "${TARGET_CADCRAFT}/previous" ]]; then
    readlink "${TARGET_CADCRAFT}/previous" > "$RECOVERY_DIR/previous.target"
    echo "share/cadcraft/previous" >> "$RECOVERY_DIR/existed_files.txt"
  else
    touch "$RECOVERY_DIR/previous.target"
  fi

  # Record TARGET_BIN entry points with explicit state: "absent", "symlink", or "regular"
  for b_name in "cadcraft" "cadcraft-cli"; do
    local b_path="${TARGET_BIN}/${b_name}"
    if [[ -L "$b_path" ]]; then
      echo "symlink" > "$RECOVERY_DIR/bin_${b_name}.type"
      readlink "$b_path" > "$RECOVERY_DIR/bin_${b_name}.val"
      echo "bin/${b_name}" >> "$RECOVERY_DIR/existed_files.txt"
    elif [[ -f "$b_path" ]]; then
      echo "regular" > "$RECOVERY_DIR/bin_${b_name}.type"
      cp -p "$b_path" "$RECOVERY_DIR/bin_${b_name}.file"
      echo "bin/${b_name}" >> "$RECOVERY_DIR/existed_files.txt"
    else
      echo "absent" > "$RECOVERY_DIR/bin_${b_name}.type"
    fi
  done

  # Backup metadata, manifest, desktop and mime files if existing
  for cand in \
    "share/applications/ai.storyteller.cadcraft.desktop" \
    "share/mime/packages/ai.storyteller.cadcraft.xml" \
    "share/metainfo/ai.storyteller.cadcraft.metainfo.xml" \
    "share/cadcraft/version.env" \
    "share/cadcraft/install-manifest.txt" \
    "share/cadcraft/uninstall.sh" \
    "share/cadcraft/install.sh"
  do
    if [[ -f "$TARGET_ROOT/$cand" ]]; then
      mkdir -p "$(dirname "$RECOVERY_DIR/files/$cand")"
      cp -p "$TARGET_ROOT/$cand" "$RECOVERY_DIR/files/$cand"
      echo "$cand" >> "$RECOVERY_DIR/existed_files.txt"
    fi
  done

  # Backup icons if present in source package and installed target
  if [[ -n "$SRC_DIR" && -d "$SRC_DIR/share/icons/hicolor" ]]; then
    while read -r icon_rel; do
      if [[ -f "$TARGET_ROOT/$icon_rel" ]]; then
        mkdir -p "$(dirname "$RECOVERY_DIR/files/$icon_rel")"
        cp -p "$TARGET_ROOT/$icon_rel" "$RECOVERY_DIR/files/$icon_rel"
        echo "$icon_rel" >> "$RECOVERY_DIR/existed_files.txt"
      fi
    done < <(cd "$SRC_DIR" && find share/icons/hicolor -type f 2>/dev/null || true)
  fi

  # Backup doc if present in source package and installed target
  if [[ -n "$SRC_DIR" && -d "$SRC_DIR/share/doc/cadcraft" ]]; then
    while read -r doc_rel; do
      if [[ -f "$TARGET_ROOT/$doc_rel" ]]; then
        mkdir -p "$(dirname "$RECOVERY_DIR/files/$doc_rel")"
        cp -p "$TARGET_ROOT/$doc_rel" "$RECOVERY_DIR/files/$doc_rel"
        echo "$doc_rel" >> "$RECOVERY_DIR/existed_files.txt"
      fi
    done < <(cd "$SRC_DIR" && find share/doc/cadcraft -type f 2>/dev/null || true)
  fi
}

# -----------------------------------------------------------------------------
# Mode: Explicit Rollback
# -----------------------------------------------------------------------------
if [[ "$IS_ROLLBACK" -eq 1 ]]; then
  acquire_lock
  check_cadcraft_running

  PREV_SYMLINK="${TARGET_CADCRAFT}/previous"
  CURR_SYMLINK="${TARGET_CADCRAFT}/current"

  if [[ ! -L "$PREV_SYMLINK" ]]; then
    echo "error: no previous version symlink found at $PREV_SYMLINK to roll back to." >&2
    exit 1
  fi

  PREV_TARGET="$(readlink "$PREV_SYMLINK" 2>/dev/null || true)"
  if [[ -z "$PREV_TARGET" ]]; then
    echo "error: previous version symlink is empty." >&2
    exit 1
  fi

  if [[ "$PREV_TARGET" == /* ]]; then
    PREV_DIR="$PREV_TARGET"
  else
    PREV_DIR="${TARGET_CADCRAFT}/${PREV_TARGET}"
  fi

  # Strict containment check: previous must resolve strictly inside versions/
  CANON_PREV="$(readlink -f "$PREV_DIR" 2>/dev/null || true)"
  CANON_VERSIONS="$(mkdir -p "$TARGET_VERSIONS" 2>/dev/null && cd "$TARGET_VERSIONS" && pwd)"
  if [[ -z "$CANON_PREV" || "$(dirname "$CANON_PREV")" != "$CANON_VERSIONS" ]]; then
    echo "error: previous version symlink target '$PREV_TARGET' resolves outside versions directory: $CANON_PREV" >&2
    exit 1
  fi

  if [[ ! -d "$PREV_DIR" || ! -f "$PREV_DIR/bin/cadcraft" ]]; then
    echo "error: previous version directory '$PREV_DIR' is invalid or missing bin/cadcraft." >&2
    exit 1
  fi

  CURR_TARGET="$(readlink "$CURR_SYMLINK" 2>/dev/null || true)"
  if [[ "$PREV_TARGET" == "$CURR_TARGET" ]]; then
    echo "error: previous version is already the active version ($CURR_TARGET)." >&2
    exit 1
  fi

  parse_legacy_version_env "$PREV_DIR/version.env"
  ROLLBACK_VERSION="$PARSED_VERSION"
  ROLLBACK_SHA="$PARSED_SHA"

  echo "=================================================================="
  echo " CADCraft — Rolling Back Installation"
  echo "=================================================================="
  echo "Current Version:   ${CURR_TARGET:-unknown}"
  echo "Rollback Target:   $PREV_TARGET ($ROLLBACK_VERSION, ${ROLLBACK_SHA:0:10})"
  echo "Destination:       $PREFIX"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "Mode:              DRY RUN (no changes will be made)"
  fi
  echo "=================================================================="

  if [[ "$NON_INTERACTIVE" -eq 0 && "$DRY_RUN" -eq 0 ]]; then
    read -r -p "Roll back to $PREV_TARGET? [y/N] " CONFIRM || CONFIRM="n"
    if [[ ! "$CONFIRM" =~ ^[yY] ]]; then
      echo "Rollback cancelled."
      exit 0
    fi
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] Would point $PREV_SYMLINK to $CURR_TARGET"
    echo "[dry-run] Would point $CURR_SYMLINK to $PREV_TARGET"
    echo "[dry-run] Would point $TARGET_BIN/cadcraft to ../share/cadcraft/current/bin/cadcraft"
    echo "[dry-run] Would update manifest and refresh desktop databases"
    exit 0
  fi

  snapshot_state_for_recovery

  # Step 1: Update 'previous' to point to old current
  if [[ -n "$CURR_TARGET" ]]; then
    PREV_TMP="${TARGET_CADCRAFT}/previous.tmp.$$"
    ln -s "$CURR_TARGET" "$PREV_TMP"
    mv -Tf "$PREV_TMP" "$PREV_SYMLINK"
  fi

  if [[ "${CADCRAFT_TEST_FAIL_DURING_ROLLBACK:-0}" -eq 1 ]]; then
    echo "test injection: simulating failure during rollback" >&2
    exit 96
  fi

  # Step 2: Update 'current' to point to rollback target
  CURR_TMP="${TARGET_CADCRAFT}/current.tmp.$$"
  ln -s "$PREV_TARGET" "$CURR_TMP"
  mv -Tf "$CURR_TMP" "$CURR_SYMLINK"

  # Step 3: Refresh entry binaries in TARGET_BIN
  mkdir -p "$TARGET_BIN"
  BIN_TMP="${TARGET_BIN}/cadcraft.tmp.$$"
  ln -s "../share/cadcraft/current/bin/cadcraft" "$BIN_TMP"
  mv -Tf "$BIN_TMP" "${TARGET_BIN}/cadcraft"

  if [[ -f "$PREV_DIR/bin/cadcraft-cli" ]]; then
    CLI_TMP="${TARGET_BIN}/cadcraft-cli.tmp.$$"
    ln -s "../share/cadcraft/current/bin/cadcraft-cli" "$CLI_TMP"
    mv -Tf "$CLI_TMP" "${TARGET_BIN}/cadcraft-cli"
  else
    rm -f "${TARGET_BIN}/cadcraft-cli" 2>/dev/null || true
  fi

  # Step 4: Update active version.env
  cp -f "$PREV_DIR/version.env" "${TARGET_CADCRAFT}/version.env" 2>/dev/null || true

  # Step 5: Refresh desktop and mime databases
  refresh_desktop_databases

  TRANSACTION_COMMITTED=1

  echo ""
  echo "Rollback complete!"
  echo "Active version is now: $PREV_TARGET"
  echo ""
  echo "Notice:"
  echo "  - Your personal settings (~/.config/cadcraft) and user drawings were NOT modified."
  echo "  - Important: Rolling back the application binaries does not revert configuration"
  echo "    or drawing schema migrations made by a newer version."
  echo "=================================================================="
  exit 0
fi

# -----------------------------------------------------------------------------
# Mode: Normal Installation / Local Update
# -----------------------------------------------------------------------------
if [[ -z "$SRC_DIR" ]]; then
  echo "error: install.sh must be run from an extracted CADCraft package directory." >&2
  exit 1
fi

if [[ ! -f "$SRC_DIR/bin/cadcraft" ]]; then
  echo "error: $SRC_DIR/bin/cadcraft not found. Invalid package directory." >&2
  exit 1
fi

acquire_lock
check_cadcraft_running

# Parse metadata from package with strict 40-character hex SHA requirement
parse_package_version_env "$SRC_DIR/version.env"
PKG_VERSION="$PARSED_VERSION"
PKG_SHA="$PARSED_SHA"
PKG_DATE="$PARSED_DATE"
PKG_ARCH="$PARSED_ARCH"
BUILD_ID="${PKG_VERSION}-${PKG_SHA}"
TARGET_BUILD_DIR="${TARGET_VERSIONS}/${BUILD_ID}"

# -----------------------------------------------------------------------------
# Full Content Verification on Existing Same-Identity Build
# -----------------------------------------------------------------------------
SAME_BUILD_EXISTS=0
ALREADY_ACTIVE=0

if [[ -d "$TARGET_BUILD_DIR" ]]; then
  SAME_BUILD_EXISTS=1

  # Compare main binary
  SRC_BIN_SHA="$(compute_sha256 "$SRC_DIR/bin/cadcraft")"
  DEST_BIN_SHA=""
  [[ -f "$TARGET_BUILD_DIR/bin/cadcraft" ]] && DEST_BIN_SHA="$(compute_sha256 "$TARGET_BUILD_DIR/bin/cadcraft")"

  # Compare CLI if present
  SRC_CLI_SHA=""
  DEST_CLI_SHA=""
  [[ -f "$SRC_DIR/bin/cadcraft-cli" ]] && SRC_CLI_SHA="$(compute_sha256 "$SRC_DIR/bin/cadcraft-cli")"
  [[ -f "$TARGET_BUILD_DIR/bin/cadcraft-cli" ]] && DEST_CLI_SHA="$(compute_sha256 "$TARGET_BUILD_DIR/bin/cadcraft-cli")"

  CONTENT_MATCH=1
  [[ "$SRC_BIN_SHA" != "$DEST_BIN_SHA" ]] && CONTENT_MATCH=0
  [[ "$SRC_CLI_SHA" != "$DEST_CLI_SHA" ]] && CONTENT_MATCH=0

  # Verify full build inventory if build-manifest.txt is present
  if [[ "$CONTENT_MATCH" -eq 1 && -f "$TARGET_BUILD_DIR/build-manifest.txt" ]]; then
    while read -r exp_sha rel_path; do
      [[ -z "$rel_path" || "$exp_sha" =~ ^# ]] && continue
      dest_item="$TARGET_BUILD_DIR/$rel_path"
      if [[ ! -f "$dest_item" ]]; then
        CONTENT_MATCH=0
        break
      fi
      cur_sha="$(compute_sha256 "$dest_item")"
      if [[ "$cur_sha" != "$exp_sha" ]]; then
        CONTENT_MATCH=0
        break
      fi
    done < "$TARGET_BUILD_DIR/build-manifest.txt"
  fi

  if [[ "$CONTENT_MATCH" -eq 0 ]]; then
    echo "error: version directory for '$BUILD_ID' already exists with DIFFERENT content!" >&2
    echo "  cadcraft source SHA:      $SRC_BIN_SHA" >&2
    echo "  cadcraft installed SHA:   $DEST_BIN_SHA" >&2
    echo "  cadcraft-cli source SHA:    ${SRC_CLI_SHA:-none}" >&2
    echo "  cadcraft-cli installed SHA: ${DEST_CLI_SHA:-none}" >&2
    echo "Refusing to overwrite existing build with same identity but different content, even with --force." >&2
    exit 1
  else
    if [[ -L "${TARGET_CADCRAFT}/current" && "$(readlink "${TARGET_CADCRAFT}/current")" == "versions/${BUILD_ID}" ]]; then
      ALREADY_ACTIVE=1
    fi
  fi
fi

# -----------------------------------------------------------------------------
# Collision Detection: Protect against overwriting unmanaged external files
# -----------------------------------------------------------------------------
EXISTING_MANIFEST="${TARGET_CADCRAFT}/install-manifest.txt"
PREVIOUS_MANAGED_FILES=()
if [[ -f "$EXISTING_MANIFEST" ]]; then
  # Verify manifest itself does not escape prefix if it is a symlink
  if [[ -L "$EXISTING_MANIFEST" ]]; then
    manifest_canon="$(readlink -f "$EXISTING_MANIFEST" 2>/dev/null || true)"
    if [[ -z "$manifest_canon" || ( "$manifest_canon" != "$CANON_ROOT" && "$manifest_canon" != "$CANON_ROOT/"* ) ]]; then
      echo "error: install-manifest.txt is an unresolvable or escaping symlink: $manifest_canon" >&2
      exit 1
    fi
  fi

  IS_UPDATE=1
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="$(echo "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    [[ -z "$line" || "$line" =~ ^# ]] && continue

    # Validate path format: must be strictly relative without '..' traversal
    if [[ "$line" == /* || "$line" == *".."* ]]; then
      echo "error: install-manifest.txt contains uncontained or invalid entry: $line" >&2
      exit 1
    fi

    target_entry="${TARGET_ROOT}/${line}"

    if [[ ! -e "$target_entry" && ! -L "$target_entry" ]]; then
      # Archivo gestionado que ya no existe: verificar que su directorio padre exista,
      # sea canónicamente verificable y esté contenido dentro del prefijo
      parent_dir="$(dirname "$target_entry")"
      canon_parent=""
      if [[ -d "$parent_dir" ]]; then
        canon_parent="$(readlink -f "$parent_dir" 2>/dev/null || true)"
      fi
      if [[ -n "$canon_parent" && ( "$canon_parent" == "$CANON_ROOT" || "$canon_parent" == "$CANON_ROOT/"* ) ]]; then
        PREVIOUS_MANAGED_FILES+=("$line")
      else
        echo "error: install-manifest.txt entry cannot be verified (parent directory missing, unresolvable or escaping): $line" >&2
        exit 1
      fi
    elif [[ -L "$target_entry" ]]; then
      # Enlace simbólico existente: debe poder resolverse dentro del prefijo (no roto ni en bucle)
      canon_entry="$(readlink -e "$target_entry" 2>/dev/null || true)"
      if [[ -z "$canon_entry" ]]; then
        echo "error: install-manifest.txt entry is an unresolvable or broken symlink: $line" >&2
        exit 1
      elif [[ "$canon_entry" != "$CANON_ROOT" && "$canon_entry" != "$CANON_ROOT/"* ]]; then
        echo "error: install-manifest.txt entry resolves outside installation prefix: $line (resolves to: $canon_entry)" >&2
        exit 1
      else
        PREVIOUS_MANAGED_FILES+=("$line")
      fi
    else
      # Archivo o directorio regular existente: verificar contención canónica
      canon_entry="$(readlink -f "$target_entry" 2>/dev/null || true)"
      if [[ -z "$canon_entry" ]]; then
        echo "error: install-manifest.txt entry cannot be canonically verified: $line" >&2
        exit 1
      elif [[ "$canon_entry" != "$CANON_ROOT" && "$canon_entry" != "$CANON_ROOT/"* ]]; then
        echo "error: install-manifest.txt entry resolves outside installation prefix: $line (resolves to: $canon_entry)" >&2
        exit 1
      else
        PREVIOUS_MANAGED_FILES+=("$line")
      fi
    fi
  done < "$EXISTING_MANIFEST"
fi

CANDIDATE_OUTSIDE_FILES=("bin/cadcraft")
if [[ -f "$SRC_DIR/bin/cadcraft-cli" || -f "$TARGET_BIN/cadcraft-cli" ]]; then
  CANDIDATE_OUTSIDE_FILES+=("bin/cadcraft-cli")
fi
if [[ -f "$SRC_DIR/share/applications/ai.storyteller.cadcraft.desktop" ]]; then
  CANDIDATE_OUTSIDE_FILES+=("share/applications/ai.storyteller.cadcraft.desktop")
fi
if [[ -d "$SRC_DIR/share/icons/hicolor" ]]; then
  while read -r icon_rel; do
    CANDIDATE_OUTSIDE_FILES+=("$icon_rel")
  done < <(cd "$SRC_DIR" && find share/icons/hicolor -type f)
fi
if [[ -f "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" ]]; then
  CANDIDATE_OUTSIDE_FILES+=("share/mime/packages/ai.storyteller.cadcraft.xml")
fi
if [[ -f "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" ]]; then
  CANDIDATE_OUTSIDE_FILES+=("share/metainfo/ai.storyteller.cadcraft.metainfo.xml")
fi
if [[ -d "$SRC_DIR/share/doc/cadcraft" ]]; then
  while read -r doc_rel; do
    CANDIDATE_OUTSIDE_FILES+=("$doc_rel")
  done < <(cd "$SRC_DIR" && find share/doc/cadcraft -type f)
fi
CANDIDATE_OUTSIDE_FILES+=("share/cadcraft/uninstall.sh")
CANDIDATE_OUTSIDE_FILES+=("share/cadcraft/install.sh")
CANDIDATE_OUTSIDE_FILES+=("share/cadcraft/version.env")
CANDIDATE_OUTSIDE_FILES+=("share/cadcraft/install-manifest.txt")
CANDIDATE_OUTSIDE_FILES+=("share/cadcraft/current")

# Unconditional containment check: ANY destination file/symlink resolving outside CANON_ROOT
# or that cannot be resolved (broken link / loop) aborts immediately, EVEN WITH --force
ESCAPING_COLLISIONS=()
UNRESOLVABLE_COLLISIONS=()
for cand in "${CANDIDATE_OUTSIDE_FILES[@]}"; do
  target_path="${TARGET_ROOT}/${cand}"
  if [[ -L "$target_path" ]]; then
    canon_target="$(readlink -e "$target_path" 2>/dev/null || true)"
    if [[ -z "$canon_target" ]]; then
      UNRESOLVABLE_COLLISIONS+=("$cand")
    elif [[ "$canon_target" != "$CANON_ROOT" && "$canon_target" != "$CANON_ROOT/"* ]]; then
      ESCAPING_COLLISIONS+=("$cand (resolves to: $canon_target)")
    fi
  fi
done

if [[ ${#UNRESOLVABLE_COLLISIONS[@]} -gt 0 ]]; then
  echo "error: destination symlink(s) cannot be resolved (broken link or loop):" >&2
  for uc in "${UNRESOLVABLE_COLLISIONS[@]}"; do
    echo "  - $TARGET_ROOT/$uc" >&2
  done
  echo "Refusing to overwrite unresolvable symlinks, even with --force." >&2
  exit 1
fi

if [[ ${#ESCAPING_COLLISIONS[@]} -gt 0 ]]; then
  echo "error: destination symlink(s) resolve outside installation prefix:" >&2
  for ec in "${ESCAPING_COLLISIONS[@]}"; do
    echo "  - $TARGET_ROOT/$ec" >&2
  done
  echo "Refusing to overwrite destinations resolving outside installation prefix, even with --force." >&2
  exit 1
fi

if [[ "$FORCE" -eq 0 ]]; then
  COLLISIONS=()
  for cand in "${CANDIDATE_OUTSIDE_FILES[@]}"; do
    target_path="${TARGET_ROOT}/${cand}"
    if [[ -e "$target_path" || -L "$target_path" ]]; then
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
    echo "These files already exist and are not accredited by any existing CADCraft manifest." >&2
    echo "Refusing to overwrite unmanaged files. Use --force to override." >&2
    exit 1
  fi
fi

echo "=================================================================="
if [[ "$IS_UPDATE" -eq 1 ]]; then
  echo " CADCraft — Updating Installation"
else
  echo " CADCraft — User Installation"
fi
echo "=================================================================="
echo "Version:      $PKG_VERSION (${PKG_SHA:0:10})"
echo "Build ID:     $BUILD_ID"
echo "Architecture: $PKG_ARCH"
echo "Build Date:   $PKG_DATE"
echo "Source:       $SRC_DIR"
echo "Destination:  $PREFIX"
if [[ -n "$DESTDIR" ]]; then
  echo "DestDir root: $DESTDIR"
fi
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "Mode:         DRY RUN (no changes will be made)"
fi
echo "=================================================================="

# User confirmation requested BEFORE any mutations
if [[ "$NON_INTERACTIVE" -eq 0 && "$DRY_RUN" -eq 0 ]]; then
  read -r -p "Proceed with installation? [Y/n] " CONFIRM || CONFIRM="n"
  if [[ "$CONFIRM" =~ ^[nN] ]]; then
    echo "Installation cancelled by user."
    exit 0
  fi
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[dry-run] Prevalidation passed. Would stage build $BUILD_ID and update installation."
  exit 0
fi

# Snapshot state for rollback recovery before any mutations
snapshot_state_for_recovery

# -----------------------------------------------------------------------------
# Staging and Mandatory Pre-Activation Smoke Tests
# -----------------------------------------------------------------------------
mkdir -p "$TARGET_VERSIONS"
mkdir -p "$TARGET_BIN"
mkdir -p "$TARGET_APPS"
mkdir -p "$TARGET_ICONS"
mkdir -p "$TARGET_MIME/packages"
mkdir -p "$TARGET_METAINFO"
mkdir -p "$TARGET_DOC"

STAGE_DIR="${TARGET_VERSIONS}/${BUILD_ID}.staging.$$"
rm -rf "$STAGE_DIR"

if [[ "$SAME_BUILD_EXISTS" -eq 0 ]]; then
  mkdir -p "$STAGE_DIR/bin"
  install -m 755 "$SRC_DIR/bin/cadcraft" "$STAGE_DIR/bin/cadcraft"

  if [[ -f "$SRC_DIR/bin/cadcraft-cli" ]]; then
    install -m 755 "$SRC_DIR/bin/cadcraft-cli" "$STAGE_DIR/bin/cadcraft-cli"
  fi

  if [[ -d "$SRC_DIR/share/doc/cadcraft" ]]; then
    mkdir -p "$STAGE_DIR/share/doc/cadcraft"
    (cd "$SRC_DIR" && find share/doc/cadcraft -type f) | while read -r doc_rel; do
      doc_dest="$STAGE_DIR/$doc_rel"
      mkdir -p "$(dirname "$doc_dest")"
      install -m 644 "$SRC_DIR/$doc_rel" "$doc_dest"
    done
  fi

  cat > "$STAGE_DIR/version.env" <<EOF
CADCRAFT_VERSION="$PKG_VERSION"
CADCRAFT_BUILD_SHA="$PKG_SHA"
CADCRAFT_BUILD_DATE="$PKG_DATE"
CADCRAFT_ARCH="$PKG_ARCH"
CADCRAFT_BUILD_ID="$BUILD_ID"
EOF

  # Pre-activation validation
  if [[ ! -s "$STAGE_DIR/bin/cadcraft" || ! -x "$STAGE_DIR/bin/cadcraft" ]]; then
    echo "error: validation failed: $STAGE_DIR/bin/cadcraft is missing, empty, or not executable." >&2
    exit 1
  fi

  # Mandatory smoke tests with timeout before modifying installation
  if ! run_with_timeout 10 "$STAGE_DIR/bin/cadcraft" --version >/dev/null 2>&1; then
    echo "error: mandatory smoke test failed: '$STAGE_DIR/bin/cadcraft --version' failed or timed out." >&2
    echo "Aborting installation before modifying existing active installation." >&2
    exit 1
  fi

  if [[ -f "$STAGE_DIR/bin/cadcraft-cli" ]]; then
    if ! run_with_timeout 10 "$STAGE_DIR/bin/cadcraft-cli" --version >/dev/null 2>&1; then
      echo "error: mandatory smoke test failed: '$STAGE_DIR/bin/cadcraft-cli --version' failed or timed out." >&2
      echo "Aborting installation before modifying existing active installation." >&2
      exit 1
    fi
  fi

  # Generate individual build manifest inventory
  {
    echo "# CADCraft build inventory: $BUILD_ID"
    echo "$(compute_sha256 "$STAGE_DIR/bin/cadcraft")  bin/cadcraft"
    if [[ -f "$STAGE_DIR/bin/cadcraft-cli" ]]; then
      echo "$(compute_sha256 "$STAGE_DIR/bin/cadcraft-cli")  bin/cadcraft-cli"
    fi
    echo "$(compute_sha256 "$STAGE_DIR/version.env")  version.env"
    if [[ -d "$STAGE_DIR/share/doc/cadcraft" ]]; then
      while read -r df; do
        echo "$(compute_sha256 "$STAGE_DIR/$df")  $df"
      done < <(cd "$STAGE_DIR" && find share/doc/cadcraft -type f)
    fi
  } > "$STAGE_DIR/build-manifest.txt"

  rm -rf "$TARGET_BUILD_DIR"
  mv -Tf "$STAGE_DIR" "$TARGET_BUILD_DIR"
  NEW_BUILD_DIR_CREATED=1
fi

# -----------------------------------------------------------------------------
# Migration from legacy flat manifest installation (executed AFTER prevalidation)
# -----------------------------------------------------------------------------
if [[ -f "$EXISTING_MANIFEST" && ! -L "${TARGET_CADCRAFT}/current" ]]; then
  echo "Migrating accredited legacy files to versioned layout..."
  parse_legacy_version_env "${TARGET_CADCRAFT}/version.env"
  LEGACY_VER="${PARSED_VERSION:-legacy}"
  LEGACY_SHA="${PARSED_SHA:-unknown}"
  LEGACY_BUILD_ID="${LEGACY_VER}-${LEGACY_SHA}"
  LEGACY_DIR="${TARGET_VERSIONS}/${LEGACY_BUILD_ID}"
  MIGRATED_LEGACY_DIR="$LEGACY_DIR"

  if [[ "${CADCRAFT_TEST_FAIL_AT_MIGRATION:-0}" -eq 1 ]]; then
    echo "test injection: simulating failure during migration" >&2
    exit 99
  fi

  mkdir -p "${LEGACY_DIR}/bin"

  # Migrate ONLY accredited files recorded in previous manifest
  for mf in "${PREVIOUS_MANAGED_FILES[@]}"; do
    case "$mf" in
      bin/cadcraft)
        if [[ -f "${TARGET_ROOT}/bin/cadcraft" && ! -L "${TARGET_ROOT}/bin/cadcraft" ]]; then
          cp -p "${TARGET_ROOT}/bin/cadcraft" "${LEGACY_DIR}/bin/cadcraft"
        fi
        ;;
      bin/cadcraft-cli)
        if [[ -f "${TARGET_ROOT}/bin/cadcraft-cli" && ! -L "${TARGET_ROOT}/bin/cadcraft-cli" ]]; then
          cp -p "${TARGET_ROOT}/bin/cadcraft-cli" "${LEGACY_DIR}/bin/cadcraft-cli"
        fi
        ;;
      share/doc/cadcraft/*)
        doc_rel="${mf#share/doc/cadcraft/}"
        if [[ -f "${TARGET_ROOT}/$mf" ]]; then
          mkdir -p "${LEGACY_DIR}/share/doc/cadcraft/$(dirname "$doc_rel")"
          cp -p "${TARGET_ROOT}/$mf" "${LEGACY_DIR}/share/doc/cadcraft/$doc_rel"
        fi
        ;;
    esac
  done

  if [[ -f "${TARGET_CADCRAFT}/version.env" ]]; then
    cp -p "${TARGET_CADCRAFT}/version.env" "${LEGACY_DIR}/version.env"
  fi

  # Create legacy build inventory
  {
    echo "# CADCraft build inventory: $LEGACY_BUILD_ID"
    [[ -f "${LEGACY_DIR}/bin/cadcraft" ]] && echo "$(compute_sha256 "${LEGACY_DIR}/bin/cadcraft")  bin/cadcraft"
    [[ -f "${LEGACY_DIR}/bin/cadcraft-cli" ]] && echo "$(compute_sha256 "${LEGACY_DIR}/bin/cadcraft-cli")  bin/cadcraft-cli"
    [[ -f "${LEGACY_DIR}/version.env" ]] && echo "$(compute_sha256 "${LEGACY_DIR}/version.env")  version.env"
  } > "${LEGACY_DIR}/build-manifest.txt"

  # Set initial previous symlink to legacy
  record_created_file "share/cadcraft/previous"
  MIGRATE_PREV_TMP="${TARGET_CADCRAFT}/previous.tmp.$$"
  ln -s "versions/${LEGACY_BUILD_ID}" "$MIGRATE_PREV_TMP"
  mv -Tf "$MIGRATE_PREV_TMP" "${TARGET_CADCRAFT}/previous"
fi

# -----------------------------------------------------------------------------
# Atomic Activation: Update previous and current symlinks
# -----------------------------------------------------------------------------
OLD_CURRENT=""
if [[ -L "${TARGET_CADCRAFT}/current" ]]; then
  OLD_CURRENT="$(readlink "${TARGET_CADCRAFT}/current" 2>/dev/null || true)"
fi

# Step 1: Update 'previous' symlink if different from target build
if [[ -n "$OLD_CURRENT" && "$OLD_CURRENT" != "versions/${BUILD_ID}" && -d "${TARGET_CADCRAFT}/${OLD_CURRENT}" ]]; then
  record_created_file "share/cadcraft/previous"
  PREV_TMP="${TARGET_CADCRAFT}/previous.tmp.$$"
  ln -s "$OLD_CURRENT" "$PREV_TMP"
  mv -Tf "$PREV_TMP" "${TARGET_CADCRAFT}/previous"
fi

if [[ "${CADCRAFT_TEST_FAIL_AFTER_PREVIOUS:-0}" -eq 1 ]]; then
  echo "test injection: simulating failure after previous and before current" >&2
  exit 98
fi

# Step 2: Update 'current' symlink
record_created_file "share/cadcraft/current"
CURR_TMP="${TARGET_CADCRAFT}/current.tmp.$$"
ln -s "versions/${BUILD_ID}" "$CURR_TMP"
mv -Tf "$CURR_TMP" "${TARGET_CADCRAFT}/current"

# -----------------------------------------------------------------------------
# Entry Symlinks: Stable pointers in TARGET_BIN
# -----------------------------------------------------------------------------
record_created_file "bin/cadcraft"
BIN_TMP="${TARGET_BIN}/cadcraft.tmp.$$"
ln -s "../share/cadcraft/current/bin/cadcraft" "$BIN_TMP"
mv -Tf "$BIN_TMP" "${TARGET_BIN}/cadcraft"

if [[ -f "$TARGET_BUILD_DIR/bin/cadcraft-cli" || -f "$SRC_DIR/bin/cadcraft-cli" ]]; then
  record_created_file "bin/cadcraft-cli"
  CLI_TMP="${TARGET_BIN}/cadcraft-cli.tmp.$$"
  ln -s "../share/cadcraft/current/bin/cadcraft-cli" "$CLI_TMP"
  mv -Tf "$CLI_TMP" "${TARGET_BIN}/cadcraft-cli"
fi

if [[ "${CADCRAFT_TEST_FAIL_AFTER_CURRENT:-0}" -eq 1 ]]; then
  echo "test injection: simulating failure after current and before completing metadata/manifest" >&2
  exit 97
fi

# -----------------------------------------------------------------------------
# Desktop Integration: Launcher, Icons, MIME, and Metainfo
# -----------------------------------------------------------------------------
INSTALLED_MANIFEST_FILES=(
  "bin/cadcraft"
  "share/cadcraft/current"
)
if [[ -L "${TARGET_CADCRAFT}/previous" ]]; then
  INSTALLED_MANIFEST_FILES+=("share/cadcraft/previous")
fi
if [[ -f "${TARGET_BIN}/cadcraft-cli" ]]; then
  INSTALLED_MANIFEST_FILES+=("bin/cadcraft-cli")
fi

DESKTOP_SRC="$SRC_DIR/share/applications/ai.storyteller.cadcraft.desktop"
if [[ -f "$DESKTOP_SRC" ]]; then
  DESKTOP_STAGE="${TARGET_CADCRAFT}/cadcraft.desktop.tmp.$$"
  ESCAPED_PREFIX="${PREFIX//\"/\\\"}"
  sed \
    -e "s|^Exec=.*|Exec=\"${ESCAPED_PREFIX}/bin/cadcraft\" %F|" \
    -e "s|^TryExec=.*|TryExec=${ESCAPED_PREFIX}/bin/cadcraft|" \
    "$DESKTOP_SRC" > "$DESKTOP_STAGE"

  record_created_file "share/applications/ai.storyteller.cadcraft.desktop"
  install -m 644 "$DESKTOP_STAGE" "$TARGET_APPS/ai.storyteller.cadcraft.desktop"
  rm -f "$DESKTOP_STAGE"
  INSTALLED_MANIFEST_FILES+=("share/applications/ai.storyteller.cadcraft.desktop")
fi

if [[ -d "$SRC_DIR/share/icons/hicolor" ]]; then
  while IFS= read -r icon_rel || [[ -n "$icon_rel" ]]; do
    [[ -z "$icon_rel" ]] && continue
    icon_dest="$TARGET_ROOT/$icon_rel"
    mkdir -p "$(dirname "$icon_dest")"
    record_created_file "$icon_rel"
    install -m 644 "$SRC_DIR/$icon_rel" "$icon_dest"
    INSTALLED_MANIFEST_FILES+=("$icon_rel")
  done < <(cd "$SRC_DIR" && find share/icons/hicolor -type f)
fi

if [[ -f "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" ]]; then
  record_created_file "share/mime/packages/ai.storyteller.cadcraft.xml"
  install -m 644 "$SRC_DIR/share/mime/packages/ai.storyteller.cadcraft.xml" "$TARGET_MIME/packages/ai.storyteller.cadcraft.xml"
  INSTALLED_MANIFEST_FILES+=("share/mime/packages/ai.storyteller.cadcraft.xml")
fi

if [[ -f "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" ]]; then
  record_created_file "share/metainfo/ai.storyteller.cadcraft.metainfo.xml"
  install -m 644 "$SRC_DIR/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" "$TARGET_METAINFO/ai.storyteller.cadcraft.metainfo.xml"
  INSTALLED_MANIFEST_FILES+=("share/metainfo/ai.storyteller.cadcraft.metainfo.xml")
fi

if [[ -d "$SRC_DIR/share/doc/cadcraft" ]]; then
  while IFS= read -r doc_rel || [[ -n "$doc_rel" ]]; do
    [[ -z "$doc_rel" ]] && continue
    doc_dest="$TARGET_ROOT/$doc_rel"
    mkdir -p "$(dirname "$doc_dest")"
    record_created_file "$doc_rel"
    install -m 644 "$SRC_DIR/$doc_rel" "$doc_dest"
    INSTALLED_MANIFEST_FILES+=("$doc_rel")
  done < <(cd "$SRC_DIR" && find share/doc/cadcraft -type f)
fi

if [[ -f "$SRC_DIR/uninstall.sh" ]]; then
  record_created_file "share/cadcraft/uninstall.sh"
  install -m 755 "$SRC_DIR/uninstall.sh" "$TARGET_CADCRAFT/uninstall.sh"
  INSTALLED_MANIFEST_FILES+=("share/cadcraft/uninstall.sh")
fi
if [[ -f "$SRC_DIR/install.sh" ]]; then
  record_created_file "share/cadcraft/install.sh"
  install -m 755 "$SRC_DIR/install.sh" "$TARGET_CADCRAFT/install.sh"
  INSTALLED_MANIFEST_FILES+=("share/cadcraft/install.sh")
fi

record_created_file "share/cadcraft/version.env"
cat > "$TARGET_CADCRAFT/version.env" <<EOF
CADCRAFT_VERSION="$PKG_VERSION"
CADCRAFT_BUILD_SHA="$PKG_SHA"
CADCRAFT_BUILD_DATE="$PKG_DATE"
CADCRAFT_ARCH="$PKG_ARCH"
CADCRAFT_BUILD_ID="$BUILD_ID"
CADCRAFT_INSTALL_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
CADCRAFT_PREFIX="$PREFIX"
EOF
INSTALLED_MANIFEST_FILES+=("share/cadcraft/version.env")

if [[ "${CADCRAFT_TEST_FAIL_AFTER_SHARED_RESOURCES:-0}" -eq 1 ]]; then
  echo "test injection: simulating failure after shared resources creation" >&2
  exit 95
fi

# -----------------------------------------------------------------------------
# Explicit build manifest entries (DO NOT sweep whole versions/ dir with find)
# -----------------------------------------------------------------------------
# 1. Retain accredited version files from prior manifest
for prev_entry in "${PREVIOUS_MANAGED_FILES[@]}"; do
  if [[ "$prev_entry" == share/cadcraft/versions/* ]]; then
    INSTALLED_MANIFEST_FILES+=("$prev_entry")
  fi
done

# 2. Add files from this build manifest
if [[ -f "$TARGET_BUILD_DIR/build-manifest.txt" ]]; then
  while read -r b_hash b_path; do
    [[ -z "$b_path" || "$b_hash" =~ ^# ]] && continue
    INSTALLED_MANIFEST_FILES+=("share/cadcraft/versions/$BUILD_ID/$b_path")
  done < "$TARGET_BUILD_DIR/build-manifest.txt"
  INSTALLED_MANIFEST_FILES+=("share/cadcraft/versions/$BUILD_ID/build-manifest.txt")
fi

# -----------------------------------------------------------------------------
# Write Manifest
# -----------------------------------------------------------------------------
MANIFEST_TEMP="${TARGET_CADCRAFT}/manifest.tmp.$$"
{
  echo "# CADCraft installation manifest"
  echo "# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "# Prefix: $PREFIX"
  echo "# Build ID: $BUILD_ID"
  # Deduplicate manifest entries safely
  printf "%s\n" "${INSTALLED_MANIFEST_FILES[@]}" | sort -u
  echo "share/cadcraft/install-manifest.txt"
} > "$MANIFEST_TEMP"
record_created_file "share/cadcraft/install-manifest.txt"
install -m 644 "$MANIFEST_TEMP" "$TARGET_CADCRAFT/install-manifest.txt"
rm -f "$MANIFEST_TEMP"

refresh_desktop_databases

TRANSACTION_COMMITTED=1

echo ""
if [[ "$SAME_BUILD_EXISTS" -eq 1 && "$ALREADY_ACTIVE" -eq 1 ]]; then
  echo "CADCraft build $BUILD_ID is already installed and active."
  echo "Refreshed desktop integration, symlinks, and caches."
else
  echo "Installation complete!"
  echo "Active version: $BUILD_ID"
fi
echo "Files installed to: $PREFIX"
echo ""
echo "Desktop Integration:"
echo "  - Application launcher: $TARGET_APPS/ai.storyteller.cadcraft.desktop"
echo "  - Stable executable:    $PREFIX/bin/cadcraft"
echo ""
echo "Note on Desktop Menus:"
echo "  Most modern desktop environments (GNOME, KDE, XFCE) update their application"
echo "  menus automatically within a few seconds. If the icon does not appear immediately:"
echo "  - GNOME on X11: Press Alt+F2, type 'r', and press Enter."
echo "  - GNOME on Wayland: Log out and log back in, or run:"
echo "      update-desktop-database ~/.local/share/applications"
echo "  - KDE Plasma: Run 'kbuildsycoca5' (or 'kbuildsycoca6') or restart plasma."
echo ""
echo "Rollback:"
echo "  To roll back to the previous version at any time:"
if [[ "$PREFIX" == "${HOME}/.local" ]]; then
  echo "  $PREFIX/share/cadcraft/install.sh --rollback"
else
  echo "  $PREFIX/share/cadcraft/install.sh --prefix \"$PREFIX\" --rollback"
fi
echo ""
echo "Uninstallation:"
echo "  To uninstall CADCraft cleanly (preserving personal drawings and configs):"
if [[ "$PREFIX" == "${HOME}/.local" ]]; then
  echo "  $PREFIX/share/cadcraft/uninstall.sh"
else
  echo "  $PREFIX/share/cadcraft/uninstall.sh --prefix \"$PREFIX\""
fi
echo "=================================================================="

exit 0
