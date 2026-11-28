#!/usr/bin/env bash
# test-install.sh — Isolated test suite for CADCraft Linux installer (Etapa A - Bloque 1)
#
# Tests pre-validation, strict 40-character hex SHA metadata, no fallbacks,
# required SHA-256 tool, no process bypass with --force, cadcraft-cli process checks,
# mandatory --version smoke tests with timeout, no automatic version pruning,
# no foreign file adoption in manifests, and uninstall process protection.
#
# NOTE: Uses mock executable binaries (ELF stubs using /usr/bin/sleep and shell stubs)
# to test installer logic safely without launching the full GUI CADCraft engine.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INSTALL_SRC="$SCRIPT_DIR/install.sh"
UNINSTALL_SRC="$SCRIPT_DIR/uninstall.sh"
DESKTOP_SRC="$SCRIPT_DIR/ai.storyteller.cadcraft.desktop"
MIME_SRC="$SCRIPT_DIR/ai.storyteller.cadcraft.mime.xml"

TEST_BASE="$(mktemp -d "${TMPDIR:-/tmp}/cadcraft-installer-test.XXXXXX")"
echo "Running isolated installer tests in: $TEST_BASE"
echo "NOTE: Testing utilizes mock executable binaries to validate installation logic without launching full GUI engine."

cleanup() {
  pkill -P $$ 2>/dev/null || true
  rm -rf "$TEST_BASE"
  echo "Cleaned up $TEST_BASE."
}
trap cleanup EXIT

# Helper to create a synthetic CADCraft tarball-style package
create_mock_package() {
  local pkg_dir="$1"
  local version="$2"
  local sha="$3"
  local bin_content="$4"

  mkdir -p "$pkg_dir/bin"
  mkdir -p "$pkg_dir/share/applications"
  mkdir -p "$pkg_dir/share/icons/hicolor/48x48/apps"
  mkdir -p "$pkg_dir/share/mime/packages"
  mkdir -p "$pkg_dir/share/metainfo"
  mkdir -p "$pkg_dir/share/doc/cadcraft"

  cat > "$pkg_dir/bin/axialcad" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  echo "axialcad $version ($sha)"
  exit 0
fi
echo "$bin_content"
EOF
  chmod 755 "$pkg_dir/bin/axialcad"
  ln -sf "axialcad" "$pkg_dir/bin/cadcraft"

  cat > "$pkg_dir/bin/axialcad-cli" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  echo "axialcad-cli $version ($sha)"
  exit 0
fi
echo "$bin_content"
EOF
  chmod 755 "$pkg_dir/bin/axialcad-cli"
  ln -sf "axialcad-cli" "$pkg_dir/bin/cadcraft-cli"

  cp "$INSTALL_SRC" "$pkg_dir/install.sh"
  cp "$UNINSTALL_SRC" "$pkg_dir/uninstall.sh"
  cp "$DESKTOP_SRC" "$pkg_dir/share/applications/ai.storyteller.cadcraft.desktop"
  cp "$MIME_SRC" "$pkg_dir/share/mime/packages/ai.storyteller.cadcraft.xml"
  echo "mock icon" > "$pkg_dir/share/icons/hicolor/48x48/apps/ai.storyteller.cadcraft.png"
  echo "mock doc" > "$pkg_dir/share/doc/cadcraft/README.md"

  cat > "$pkg_dir/version.env" <<EOF
CADCRAFT_VERSION="$version"
CADCRAFT_BUILD_SHA="$sha"
CADCRAFT_BUILD_DATE="2026-11-02T12:00:00Z"
CADCRAFT_ARCH="x86_64"
EOF
}

PASSED_COUNT=0
FAILED_COUNT=0

assert_eq() {
  local expected="$1"
  local actual="$2"
  local msg="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "  [FAIL] $msg: expected '$expected', got '$actual'" >&2
    FAILED_COUNT=$((FAILED_COUNT + 1))
    return 1
  else
    echo "  [PASS] $msg"
    PASSED_COUNT=$((PASSED_COUNT + 1))
    return 0
  fi
}

assert_file_exists() {
  local file="$1"
  local msg="$2"
  if [[ ! -e "$file" && ! -L "$file" ]]; then
    echo "  [FAIL] $msg: '$file' does not exist" >&2
    FAILED_COUNT=$((FAILED_COUNT + 1))
    return 1
  else
    echo "  [PASS] $msg"
    PASSED_COUNT=$((PASSED_COUNT + 1))
    return 0
  fi
}

assert_file_not_exists() {
  local file="$1"
  local msg="$2"
  if [[ -e "$file" || -L "$file" ]]; then
    echo "  [FAIL] $msg: '$file' unexpectedly exists" >&2
    FAILED_COUNT=$((FAILED_COUNT + 1))
    return 1
  else
    echo "  [PASS] $msg"
    PASSED_COUNT=$((PASSED_COUNT + 1))
    return 0
  fi
}

# =============================================================================
# BASE SETUP: Fresh install (V1) and Update (V2) with valid 40-hex SHAs
# =============================================================================
echo ""
echo "=== PRUEBA BASE: Primera instalación (V1) y Actualización (V2) ==="
T_PREFIX="$TEST_BASE/main_prefix"
PKG_V1="$TEST_BASE/pkg_v1"
SHA_V1="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
BUILD_ID_1="0.1.0-$SHA_V1"
create_mock_package "$PKG_V1" "0.1.0" "$SHA_V1" "v1_content"

(cd "$PKG_V1" && ./install.sh --prefix "$T_PREFIX" -y) >/dev/null

assert_file_exists "$T_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad" "V1 instalado en directorio versionado axialcad"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$T_PREFIX/share/axialcad/current")" "current apunta a V1"
assert_eq "../share/axialcad/current/bin/axialcad" "$(readlink "$T_PREFIX/bin/axialcad")" "bin/axialcad apunta a current"
assert_eq "axialcad" "$(readlink "$T_PREFIX/bin/cadcraft")" "bin/cadcraft apunta a axialcad"
assert_eq "../share/axialcad/current/bin/axialcad-cli" "$(readlink "$T_PREFIX/bin/axialcad-cli")" "bin/axialcad-cli apunta a current"
assert_eq "axialcad-cli" "$(readlink "$T_PREFIX/bin/cadcraft-cli")" "bin/cadcraft-cli apunta a axialcad-cli"

# Comprobaciones de ejecución directa y compatibilidad
assert_eq "v1_content" "$("$T_PREFIX/bin/axialcad")" "Invocación directa de axialcad funciona"
assert_eq "v1_content" "$("$T_PREFIX/bin/cadcraft")" "Invocación compatible de cadcraft funciona"
assert_eq "v1_content" "$("$T_PREFIX/bin/axialcad-cli")" "Invocación directa de axialcad-cli funciona"
assert_eq "v1_content" "$("$T_PREFIX/bin/cadcraft-cli")" "Invocación compatible de cadcraft-cli funciona"

# Comprobación de lanzador desktop
grep -q 'Exec=".*bin/axialcad" %F' "$T_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" && DESK_EXEC=1 || DESK_EXEC=0
assert_eq "1" "$DESK_EXEC" "Lanzador desktop tiene Exec apuntando a axialcad"
grep -q "TryExec=.*bin/axialcad" "$T_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" && DESK_TRY=1 || DESK_TRY=0
assert_eq "1" "$DESK_TRY" "Lanzador desktop tiene TryExec apuntando a axialcad"

PKG_V2="$TEST_BASE/pkg_v2"
SHA_V2="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
BUILD_ID_2="0.1.0-$SHA_V2"
create_mock_package "$PKG_V2" "0.1.0" "$SHA_V2" "v2_content"

(cd "$PKG_V2" && ./install.sh --prefix "$T_PREFIX" --update -y) >/dev/null

assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "current conmuta a V2"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$T_PREFIX/share/axialcad/previous")" "previous apunta a V1"
assert_eq "v2_content" "$("$T_PREFIX/bin/axialcad")" "axialcad ejecuta V2 tras actualización"
assert_eq "v2_content" "$("$T_PREFIX/bin/cadcraft")" "cadcraft ejecuta V2 tras actualización"

# =============================================================================
# REQUISITO 1: Exigir SHA hexadecimal de 40 caracteres, rechazar inválidos/duplicados
# =============================================================================
echo ""
echo "=== REQUISITO 1: Validación de SHA de 40 caracteres y rechazo de metadatos inválidos ==="

# 1A: SHA corto (8 caracteres)
PKG_SHORT_SHA="$TEST_BASE/pkg_short_sha"
create_mock_package "$PKG_SHORT_SHA" "0.1.0" "12345678" "content"
RET_SHORT=0
(cd "$PKG_SHORT_SHA" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_SHORT=$?
assert_eq "1" "$RET_SHORT" "Rechazo de SHA corto (8 caracteres)"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Estado previo intacto tras rechazo de SHA corto"

# 1B: SHA con caracteres no hexadecimales (40 caracteres conteniendo 'g')
PKG_NOHEX_SHA="$TEST_BASE/pkg_nohex_sha"
create_mock_package "$PKG_NOHEX_SHA" "0.1.0" "gggggggggggggggggggggggggggggggggggggggg" "content"
RET_NOHEX=0
(cd "$PKG_NOHEX_SHA" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_NOHEX=$?
assert_eq "1" "$RET_NOHEX" "Rechazo de SHA con caracteres no hexadecimales"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Estado previo intacto tras rechazo de SHA no hex"

# 1C: Clave duplicada CADCRAFT_VERSION
PKG_DUP_KEY="$TEST_BASE/pkg_dup_key"
create_mock_package "$PKG_DUP_KEY" "0.1.0" "cccccccccccccccccccccccccccccccccccccccc" "content"
echo 'CADCRAFT_VERSION="0.1.1"' >> "$PKG_DUP_KEY/version.env"
RET_DUP=0
(cd "$PKG_DUP_KEY" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_DUP=$?
assert_eq "1" "$RET_DUP" "Rechazo de clave duplicada en version.env"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Estado previo intacto tras rechazo de clave duplicada"

# 1D: Clave obligatoria CADCRAFT_VERSION ausente
PKG_NO_VER="$TEST_BASE/pkg_no_ver"
create_mock_package "$PKG_NO_VER" "0.1.0" "cccccccccccccccccccccccccccccccccccccccc" "content"
sed -i '/CADCRAFT_VERSION/d' "$PKG_NO_VER/version.env"
RET_NOVER=0
(cd "$PKG_NO_VER" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_NOVER=$?
assert_eq "1" "$RET_NOVER" "Rechazo de paquete sin CADCRAFT_VERSION"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Estado previo intacto tras rechazo por versión ausente"

# =============================================================================
# REQUISITO 2: Exigir herramienta SHA-256 real (sin fallback a tamaño/mtime)
# =============================================================================
echo ""
echo "=== REQUISITO 2: Exigencia de herramienta SHA-256 real ==="
# Creamos un PATH temporal sin sha256sum ni shasum
MOCK_BIN_DIR="$TEST_BASE/mock_tools"
mkdir -p "$MOCK_BIN_DIR"
# Enlazamos las utilidades básicas del sistema excepto sha256sum y shasum
for tool in bash sh cat grep sed cp mv rm mkdir chmod readlink pgrep timeout cut tr sort find dirname basename date stat install flock; do
  tool_path="$(command -v "$tool" || true)"
  if [[ -n "$tool_path" ]]; then
    ln -s "$tool_path" "$MOCK_BIN_DIR/$tool" 2>/dev/null || true
  fi
done

RET_NOSHA=0
(export PATH="$MOCK_BIN_DIR"; cd "$PKG_V1" && ./install.sh --prefix "$TEST_BASE/nosha_prefix" -y) 2>/dev/null || RET_NOSHA=$?
assert_eq "1" "$RET_NOSHA" "El instalador falla si no existe una herramienta SHA-256 real"

# =============================================================================
# REQUISITO 3: Eliminar bypass con --force de procesos y comprobar axialcad y cli
# =============================================================================
echo ""
echo "=== REQUISITO 3: Prohibición de bypass con --force y comprobación de procesos ==="

# 3A: Proceso axialcad activo rechaza --update incluso con --force
TARGET_REAL_AXIALCAD="$T_PREFIX/share/axialcad/versions/$BUILD_ID_2/bin/axialcad"
cp /usr/bin/sleep "$TARGET_REAL_AXIALCAD"

bash -c "exec \"$TARGET_REAL_AXIALCAD\" 10" &
PID_AXIALCAD=$!
sleep 0.2

RET_FORCE_BYPASS=0
(cd "$PKG_V1" && ./install.sh --prefix "$T_PREFIX" --update --force -y) 2>/dev/null || RET_FORCE_BYPASS=$?
assert_eq "1" "$RET_FORCE_BYPASS" "El instalador rechaza actualizar con proceso axialcad activo incluso con --force"

# Verificar que el proceso sigue vivo
kill -0 "$PID_AXIALCAD" 2>/dev/null && STILL_ALIVE_1=1 || STILL_ALIVE_1=0
assert_eq "1" "$STILL_ALIVE_1" "El proceso axialcad NO fue terminado automáticamente"
kill -9 "$PID_AXIALCAD" 2>/dev/null || true
wait "$PID_AXIALCAD" 2>/dev/null || true

# Restaurar stub de axialcad
cat > "$TARGET_REAL_AXIALCAD" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "axialcad 0.1.0 ($SHA_V2)"; exit 0; fi
echo "v2_content"
EOF
chmod 755 "$TARGET_REAL_AXIALCAD"

# 3B: Proceso axialcad-cli activo rechaza actualización
TARGET_REAL_CLI="$T_PREFIX/share/axialcad/versions/$BUILD_ID_2/bin/axialcad-cli"
cp /usr/bin/sleep "$TARGET_REAL_CLI"

bash -c "exec \"$TARGET_REAL_CLI\" 10" &
PID_CLI=$!
sleep 0.2

RET_CLI_BLOCK=0
(cd "$PKG_V1" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_CLI_BLOCK=$?
assert_eq "1" "$RET_CLI_BLOCK" "El instalador rechaza actualizar con proceso axialcad-cli activo"

kill -0 "$PID_CLI" 2>/dev/null && STILL_ALIVE_2=1 || STILL_ALIVE_2=0
assert_eq "1" "$STILL_ALIVE_2" "El proceso axialcad-cli NO fue terminado automáticamente"
kill -9 "$PID_CLI" 2>/dev/null || true
wait "$PID_CLI" 2>/dev/null || true

# Restaurar stub de axialcad-cli
cat > "$TARGET_REAL_CLI" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "axialcad-cli 0.1.0 ($SHA_V2)"; exit 0; fi
echo "v2_content"
EOF
chmod 755 "$TARGET_REAL_CLI"

# =============================================================================
# REQUISITO 4: Smoke tests obligatorios --version con timeout
# =============================================================================
echo ""
echo "=== REQUISITO 4: Smoke tests --version obligatorios con timeout ==="

# 4A: Binario que falla al ejecutar --version
PKG_FAIL_VERSION="$TEST_BASE/pkg_fail_version"
SHA_FAIL="cccccccccccccccccccccccccccccccccccccccc"
create_mock_package "$PKG_FAIL_VERSION" "0.1.0" "$SHA_FAIL" "content"
# Modificamos bin/axialcad para que devuelva error al pedir --version
cat > "$PKG_FAIL_VERSION/bin/axialcad" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  echo "Error interno de inicialización de símbolos" >&2
  exit 127
fi
EOF
chmod 755 "$PKG_FAIL_VERSION/bin/axialcad"

RET_FAIL_VER=0
(cd "$PKG_FAIL_VERSION" && ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_FAIL_VER=$?
assert_eq "1" "$RET_FAIL_VER" "El instalador aborta si el smoke test --version falla"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Versión activa intacta tras fallo de smoke test"
assert_file_not_exists "$T_PREFIX/share/axialcad/versions/0.1.0-$SHA_FAIL" "Directorio versionado no fue creado en versions/"

# 4B: Binario que cuelga en --version (timeout)
PKG_HANG_VERSION="$TEST_BASE/pkg_hang_version"
SHA_HANG="dddddddddddddddddddddddddddddddddddddddd"
create_mock_package "$PKG_HANG_VERSION" "0.1.0" "$SHA_HANG" "content"
cat > "$PKG_HANG_VERSION/bin/axialcad" <<EOF
#!/usr/bin/env bash
sleep 30
exit 0
EOF
chmod 755 "$PKG_HANG_VERSION/bin/axialcad"

# Usamos timeout acotado para la prueba rápida invocando con run_with_timeout o timeout
RET_HANG=0
(cd "$PKG_HANG_VERSION" && timeout 12 ./install.sh --prefix "$T_PREFIX" --update -y) 2>/dev/null || RET_HANG=$?
assert_eq "1" "$RET_HANG" "El smoke test aborta por timeout cuando el binario cuelga"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$T_PREFIX/share/axialcad/current")" "Versión activa intacta tras timeout"

# =============================================================================
# REQUISITO 5: Desactivar por completo la limpieza automática de versiones
# y no adoptar archivos ajenos en el manifiesto
# =============================================================================
echo ""
echo "=== REQUISITO 5: Sin poda automática de versiones y manifiesto estricto ==="

# Instalar versión V3
PKG_V3="$TEST_BASE/pkg_v3"
SHA_V3="eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
BUILD_ID_3="0.1.0-$SHA_V3"
create_mock_package "$PKG_V3" "0.1.0" "$SHA_V3" "v3_content"

# Colocar un archivo ajeno dentro del árbol de versions antes de instalar
mkdir -p "$T_PREFIX/share/axialcad/versions"
echo "archivo de usuario o extensión externa" > "$T_PREFIX/share/axialcad/versions/external_user_plugin.txt"

(cd "$PKG_V3" && ./install.sh --prefix "$T_PREFIX" --update -y) >/dev/null

# Comprobar que V1 (versión previa a V2) NO fue podada
assert_file_exists "$T_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad" "Versión V1 no fue eliminada (poda automática desactivada)"
assert_file_exists "$T_PREFIX/share/axialcad/versions/$BUILD_ID_2/bin/axialcad" "Versión V2 conservada como previous"
assert_file_exists "$T_PREFIX/share/axialcad/versions/$BUILD_ID_3/bin/axialcad" "Versión V3 activa en current"

# Comprobar que el archivo ajeno NO fue adoptado en el manifiesto
grep -q "external_user_plugin.txt" "$T_PREFIX/share/axialcad/install-manifest.txt" && ADOPTED=1 || ADOPTED=0
assert_eq "0" "$ADOPTED" "El manifiesto NO adoptó el archivo ajeno en versions/"

# =============================================================================
# REQUISITO 6: Desinstalador con comprobación de procesos y variables corregidas
# =============================================================================
echo ""
echo "=== REQUISITO 6: Comprobación de procesos en desinstalador (variables corregidas) ==="

# Iniciar proceso axialcad desde la versión activa V3
TARGET_V3_BIN="$T_PREFIX/share/axialcad/versions/$BUILD_ID_3/bin/axialcad"
cp /usr/bin/sleep "$TARGET_V3_BIN"

bash -c "exec \"$TARGET_V3_BIN\" 10" &
PID_UNINSTALL=$!
sleep 0.2

RET_UNINSTALL=0
"$T_PREFIX/share/axialcad/uninstall.sh" --prefix "$T_PREFIX" -y 2>/dev/null || RET_UNINSTALL=$?
assert_eq "1" "$RET_UNINSTALL" "El desinstalador rechaza desinstalar con axialCAD activo"

kill -0 "$PID_UNINSTALL" 2>/dev/null && UNINSTALL_PID_ALIVE=1 || UNINSTALL_PID_ALIVE=0
assert_eq "1" "$UNINSTALL_PID_ALIVE" "El proceso activo NO fue terminado por el desinstalador"

# Comprobar que la instalación sigue intacta
assert_file_exists "$T_PREFIX/bin/axialcad" "El lanzador axialcad permanece intacto"
assert_file_exists "$T_PREFIX/bin/cadcraft" "El alias cadcraft permanece intacto"
assert_file_exists "$T_PREFIX/share/axialcad/current" "current permanece intacto"

kill -9 "$PID_UNINSTALL" 2>/dev/null || true
wait "$PID_UNINSTALL" 2>/dev/null || true

# Restaurar binario para desinstalación limpia final
cat > "$TARGET_V3_BIN" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "axialcad 0.1.0 ($SHA_V3)"; exit 0; fi
EOF
chmod 755 "$TARGET_V3_BIN"

# Desinstalación sin procesos debe tener éxito
"$T_PREFIX/share/axialcad/uninstall.sh" --prefix "$T_PREFIX" -y >/dev/null
assert_file_not_exists "$T_PREFIX/bin/axialcad" "Desinstalación exitosa: bin/axialcad retirado"
assert_file_not_exists "$T_PREFIX/bin/cadcraft" "Desinstalación exitosa: bin/cadcraft retirado"
assert_file_not_exists "$T_PREFIX/bin/axialcad-cli" "Desinstalación exitosa: bin/axialcad-cli retirado"
assert_file_not_exists "$T_PREFIX/bin/cadcraft-cli" "Desinstalación exitosa: bin/cadcraft-cli retirado"
assert_file_not_exists "$T_PREFIX/share/axialcad/current" "current retirado tras desinstalación"
assert_file_exists "$T_PREFIX/share/axialcad/versions/external_user_plugin.txt" "Archivo ajeno en versions/ preservado por el desinstalador"

# =============================================================================
# REQUISITO B2-1: Exigir flock en instalador y desinstalador
# =============================================================================
echo ""
echo "=== REQUISITO B2-1: Exigencia de flock en instalador y desinstalador ==="
MOCK_NOFLOCK_DIR="$TEST_BASE/mock_noflock"
mkdir -p "$MOCK_NOFLOCK_DIR"
for tool in bash sh cat grep sed cp mv rm mkdir chmod readlink pgrep timeout cut tr sort find dirname basename date stat install sha256sum; do
  tool_path="$(command -v "$tool" || true)"
  if [[ -n "$tool_path" ]]; then
    ln -s "$tool_path" "$MOCK_NOFLOCK_DIR/$tool" 2>/dev/null || true
  fi
done

# 1. Instalador sin flock aborta antes de modificar destino
NOFLOCK_PREFIX="$TEST_BASE/noflock_prefix"
RET_NOFLOCK_INST=0
(export PATH="$MOCK_NOFLOCK_DIR"; cd "$PKG_V1" && ./install.sh --prefix "$NOFLOCK_PREFIX" -y) 2>/dev/null || RET_NOFLOCK_INST=$?
assert_eq "1" "$RET_NOFLOCK_INST" "El instalador aborta con código 1 si falta flock"
assert_file_not_exists "$NOFLOCK_PREFIX/bin/axialcad" "El destino no fue modificado tras fallo por ausencia de flock"

# 2. Desinstalador sin flock aborta antes de modificar destino
# Preparamos una instalación válida primero con flock
FLOCK_TARGET_PREFIX="$TEST_BASE/flock_target_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$FLOCK_TARGET_PREFIX" -y) >/dev/null
assert_file_exists "$FLOCK_TARGET_PREFIX/bin/axialcad" "Instalación de prueba para flock creada"

RET_NOFLOCK_UNINST=0
(export PATH="$MOCK_NOFLOCK_DIR"; "$FLOCK_TARGET_PREFIX/share/axialcad/uninstall.sh" --prefix "$FLOCK_TARGET_PREFIX" -y) 2>/dev/null || RET_NOFLOCK_UNINST=$?
assert_eq "1" "$RET_NOFLOCK_UNINST" "El desinstalador aborta con código 1 si falta flock"
assert_file_exists "$FLOCK_TARGET_PREFIX/bin/axialcad" "El desinstalador no modificó archivos al faltar flock"

# =============================================================================
# REQUISITO B2-2: Validación de metadatos obligatorios generados por package.sh
# (versión, SHA, fecha, arquitectura) y parser explícito de migración legacy
# =============================================================================
echo ""
echo "=== REQUISITO B2-2: Validación estricta de metadatos (fecha, arch, duplicados, legacy) ==="

# 2A: Fecha inválida
PKG_INVALID_DATE="$TEST_BASE/pkg_invalid_date"
create_mock_package "$PKG_INVALID_DATE" "0.1.0" "$SHA_V1" "content"
sed -i 's/CADCRAFT_BUILD_DATE=.*/CADCRAFT_BUILD_DATE="2026\/11\/02"/' "$PKG_INVALID_DATE/version.env"
RET_INVALID_DATE=0
(cd "$PKG_INVALID_DATE" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_INVALID_DATE=$?
assert_eq "1" "$RET_INVALID_DATE" "Rechazo de fecha de compilación con formato inválido"

# 2B: Fecha ausente
PKG_NO_DATE="$TEST_BASE/pkg_no_date"
create_mock_package "$PKG_NO_DATE" "0.1.0" "$SHA_V1" "content"
sed -i '/CADCRAFT_BUILD_DATE/d' "$PKG_NO_DATE/version.env"
RET_NO_DATE=0
(cd "$PKG_NO_DATE" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_NO_DATE=$?
assert_eq "1" "$RET_NO_DATE" "Rechazo de paquete sin CADCRAFT_BUILD_DATE"

# 2C: Clave duplicada CADCRAFT_BUILD_DATE
PKG_DUP_DATE="$TEST_BASE/pkg_dup_date"
create_mock_package "$PKG_DUP_DATE" "0.1.0" "$SHA_V1" "content"
echo 'CADCRAFT_BUILD_DATE="2026-11-02T13:00:00Z"' >> "$PKG_DUP_DATE/version.env"
RET_DUP_DATE=0
(cd "$PKG_DUP_DATE" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_DUP_DATE=$?
assert_eq "1" "$RET_DUP_DATE" "Rechazo de clave duplicada CADCRAFT_BUILD_DATE"

# 2D: Arquitectura ausente
PKG_NO_ARCH="$TEST_BASE/pkg_no_arch"
create_mock_package "$PKG_NO_ARCH" "0.1.0" "$SHA_V1" "content"
sed -i '/CADCRAFT_ARCH/d' "$PKG_NO_ARCH/version.env"
RET_NO_ARCH=0
(cd "$PKG_NO_ARCH" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_NO_ARCH=$?
assert_eq "1" "$RET_NO_ARCH" "Rechazo de paquete sin CADCRAFT_ARCH"

# 2E: Arquitectura inválida con caracteres especiales
PKG_BAD_ARCH="$TEST_BASE/pkg_bad_arch"
create_mock_package "$PKG_BAD_ARCH" "0.1.0" "$SHA_V1" "content"
sed -i 's/CADCRAFT_ARCH=.*/CADCRAFT_ARCH="x86_64;rm -rf \/"/' "$PKG_BAD_ARCH/version.env"
RET_BAD_ARCH=0
(cd "$PKG_BAD_ARCH" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_BAD_ARCH=$?
assert_eq "1" "$RET_BAD_ARCH" "Rechazo de CADCRAFT_ARCH con caracteres no permitidos"

# 2F: Clave desconocida
PKG_UNKNOWN_KEY="$TEST_BASE/pkg_unknown_key"
create_mock_package "$PKG_UNKNOWN_KEY" "0.1.0" "$SHA_V1" "content"
echo 'MALICIOUS_CUSTOM_ENV="eval_me"' >> "$PKG_UNKNOWN_KEY/version.env"
RET_UNKNOWN_KEY=0
(cd "$PKG_UNKNOWN_KEY" && ./install.sh --prefix "$TEST_BASE/meta_test_prefix" -y) 2>/dev/null || RET_UNKNOWN_KEY=$?
assert_eq "1" "$RET_UNKNOWN_KEY" "Rechazo de clave desconocida en version.env"

# =============================================================================
# REQUISITO B2-3: CLI alterada con misma identidad de build aborta incluso con --force
# =============================================================================
echo ""
echo "=== REQUISITO B2-3: Alteración de CLI con misma identidad aborta incluso con --force ==="
CLI_PREFIX="$TEST_BASE/cli_alter_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$CLI_PREFIX" -y) >/dev/null
assert_file_exists "$CLI_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad-cli" "Build V1 instalado"

# Alteramos axialcad-cli dentro del build instalado
echo "# alteracion maliciosa o corrupcion en CLI" >> "$CLI_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad-cli"

RET_ALTERED_CLI=0
(cd "$PKG_V1" && ./install.sh --prefix "$CLI_PREFIX" --force -y) 2>/dev/null || RET_ALTERED_CLI=$?
assert_eq "1" "$RET_ALTERED_CLI" "Rechazo de build con misma identidad y CLI alterada, incluso con --force"

# =============================================================================
# REQUISITO B2-4: Fallo durante migración restaura instalación plana anterior
# =============================================================================
echo ""
echo "=== REQUISITO B2-4: Fallo durante migración restaura instalación plana anterior ==="
MIGRATE_FAIL_PREFIX="$TEST_BASE/migrate_fail_prefix"
mkdir -p "$MIGRATE_FAIL_PREFIX/bin"
mkdir -p "$MIGRATE_FAIL_PREFIX/share/cadcraft"
mkdir -p "$MIGRATE_FAIL_PREFIX/share/applications"

# Simular instalación plana anterior acreditada por manifiesto
cat > "$MIGRATE_FAIL_PREFIX/bin/cadcraft" <<EOF
#!/usr/bin/env bash
echo "original flat binary"
EOF
chmod 755 "$MIGRATE_FAIL_PREFIX/bin/cadcraft"

cat > "$MIGRATE_FAIL_PREFIX/share/cadcraft/version.env" <<EOF
CADCRAFT_VERSION="0.0.9"
CADCRAFT_BUILD_SHA="0000000000000000000000000000000000000000"
EOF

cat > "$MIGRATE_FAIL_PREFIX/share/cadcraft/install-manifest.txt" <<EOF
bin/cadcraft
share/cadcraft/version.env
share/cadcraft/install-manifest.txt
EOF

# Guardamos hash del binario plano original
ORIG_FLAT_HASH="$(sha256sum "$MIGRATE_FAIL_PREFIX/bin/cadcraft" | cut -d' ' -f1)"

# Inyectamos fallo controlado durante migración
RET_FAIL_MIG=0
(export CADCRAFT_TEST_FAIL_AT_MIGRATION=1; cd "$PKG_V1" && ./install.sh --prefix "$MIGRATE_FAIL_PREFIX" -y) 2>/dev/null || RET_FAIL_MIG=$?
assert_eq "99" "$RET_FAIL_MIG" "Fallo inyectado durante migración capturado correctamente"

# Comprobamos que la recuperación restauró la instalación plana
assert_file_exists "$MIGRATE_FAIL_PREFIX/bin/cadcraft" "Binario plano original conservado"
# Debe ser un archivo regular, NO un symlink
[[ ! -L "$MIGRATE_FAIL_PREFIX/bin/cadcraft" ]] && IS_REG=1 || IS_REG=0
assert_eq "1" "$IS_REG" "bin/cadcraft sigue siendo archivo regular, no symlink"
CURR_FLAT_HASH="$(sha256sum "$MIGRATE_FAIL_PREFIX/bin/cadcraft" | cut -d' ' -f1)"
assert_eq "$ORIG_FLAT_HASH" "$CURR_FLAT_HASH" "Contenido del binario plano idéntico tras recuperación"
assert_file_not_exists "$MIGRATE_FAIL_PREFIX/share/cadcraft/versions" "Directorio versions no residual tras fallo de migración"
assert_file_not_exists "$MIGRATE_FAIL_PREFIX/share/cadcraft/current" "Symlink current no existe tras aborto de migración"
assert_file_not_exists "$MIGRATE_FAIL_PREFIX/share/axialcad" "Directorio share/axialcad no creado tras fallo de migración"
assert_file_not_exists "$MIGRATE_FAIL_PREFIX/bin/axialcad" "Lanzador bin/axialcad no creado tras fallo de migración"

# =============================================================================
# REQUISITO B2-5: Fallo tras previous y antes de current
# =============================================================================
echo ""
echo "=== REQUISITO B2-5: Fallo tras previous y antes de current ==="
FAIL_PREV_PREFIX="$TEST_BASE/fail_prev_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$FAIL_PREV_PREFIX" -y) >/dev/null
assert_eq "versions/$BUILD_ID_1" "$(readlink "$FAIL_PREV_PREFIX/share/axialcad/current")" "V1 activo inicialmente"

# Inyectamos fallo tras previous y antes de current
RET_FAIL_PREV=0
(export CADCRAFT_TEST_FAIL_AFTER_PREVIOUS=1; cd "$PKG_V2" && ./install.sh --prefix "$FAIL_PREV_PREFIX" --update -y) 2>/dev/null || RET_FAIL_PREV=$?
assert_eq "98" "$RET_FAIL_PREV" "Fallo inyectado tras previous capturado correctamente"

# Verificamos estado restaurado exactamente
assert_eq "versions/$BUILD_ID_1" "$(readlink "$FAIL_PREV_PREFIX/share/axialcad/current")" "current restaurado a V1"
assert_file_not_exists "$FAIL_PREV_PREFIX/share/axialcad/previous" "previous restaurado a estado inexistente previo"
assert_file_not_exists "$FAIL_PREV_PREFIX/share/axialcad/versions/$BUILD_ID_2" "Directorio V2 retirado tras recuperación"
assert_eq "../share/axialcad/current/bin/axialcad" "$(readlink "$FAIL_PREV_PREFIX/bin/axialcad")" "bin/axialcad apunta a active current"
assert_eq "axialcad" "$(readlink "$FAIL_PREV_PREFIX/bin/cadcraft")" "bin/cadcraft apunta a axialcad"

# =============================================================================
# REQUISITO B2-6: Fallo tras current y antes de completar metadatos/manifiesto
# =============================================================================
echo ""
echo "=== REQUISITO B2-6: Fallo tras current y antes de completar metadatos/manifiesto ==="
FAIL_CURR_PREFIX="$TEST_BASE/fail_curr_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$FAIL_CURR_PREFIX" -y) >/dev/null
assert_eq "versions/$BUILD_ID_1" "$(readlink "$FAIL_CURR_PREFIX/share/axialcad/current")" "V1 activo inicialmente"

RET_FAIL_CURR=0
(export CADCRAFT_TEST_FAIL_AFTER_CURRENT=1; cd "$PKG_V2" && ./install.sh --prefix "$FAIL_CURR_PREFIX" --update -y) 2>/dev/null || RET_FAIL_CURR=$?
assert_eq "97" "$RET_FAIL_CURR" "Fallo inyectado tras current capturado correctamente"

# Verificamos recuperación completa
assert_eq "versions/$BUILD_ID_1" "$(readlink "$FAIL_CURR_PREFIX/share/axialcad/current")" "current restaurado a V1"
assert_file_not_exists "$FAIL_CURR_PREFIX/share/axialcad/previous" "previous restaurado"
assert_file_not_exists "$FAIL_CURR_PREFIX/share/axialcad/versions/$BUILD_ID_2" "Directorio V2 retirado"
# El version.env activo debe corresponder a V1
grep -q "$SHA_V1" "$FAIL_CURR_PREFIX/share/axialcad/version.env" && V1_META=1 || V1_META=0
assert_eq "1" "$V1_META" "version.env activo corresponde a V1 tras recuperación"

# Las cuatro entradas públicas en bin deben seguir apuntando a V1 y ejecutar V1
assert_eq "../share/axialcad/current/bin/axialcad" "$(readlink "$FAIL_CURR_PREFIX/bin/axialcad")" "bin/axialcad restaurado tras fallo"
assert_eq "axialcad" "$(readlink "$FAIL_CURR_PREFIX/bin/cadcraft")" "bin/cadcraft restaurado tras fallo"
assert_eq "../share/axialcad/current/bin/axialcad-cli" "$(readlink "$FAIL_CURR_PREFIX/bin/axialcad-cli")" "bin/axialcad-cli restaurado tras fallo"
assert_eq "axialcad-cli" "$(readlink "$FAIL_CURR_PREFIX/bin/cadcraft-cli")" "bin/cadcraft-cli restaurado tras fallo"
assert_eq "v1_content" "$("$FAIL_CURR_PREFIX/bin/axialcad")" "bin/axialcad ejecuta V1 tras recuperación"
assert_eq "v1_content" "$("$FAIL_CURR_PREFIX/bin/cadcraft")" "bin/cadcraft ejecuta V1 tras recuperación"
assert_eq "v1_content" "$("$FAIL_CURR_PREFIX/bin/axialcad-cli")" "bin/axialcad-cli ejecuta V1 tras recuperación"
assert_eq "v1_content" "$("$FAIL_CURR_PREFIX/bin/cadcraft-cli")" "bin/cadcraft-cli ejecuta V1 tras recuperación"

# =============================================================================
# REQUISITO B2-7: Fallo durante rollback restaura estado previo
# =============================================================================
echo ""
echo "=== REQUISITO B2-7: Fallo durante rollback restaura estado previo ==="
ROLL_FAIL_PREFIX="$TEST_BASE/roll_fail_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$ROLL_FAIL_PREFIX" -y) >/dev/null
(cd "$PKG_V2" && ./install.sh --prefix "$ROLL_FAIL_PREFIX" --update -y) >/dev/null
assert_eq "versions/$BUILD_ID_2" "$(readlink "$ROLL_FAIL_PREFIX/share/axialcad/current")" "V2 activo antes de rollback"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$ROLL_FAIL_PREFIX/share/axialcad/previous")" "V1 en previous antes de rollback"

RET_FAIL_ROLL=0
(export CADCRAFT_TEST_FAIL_DURING_ROLLBACK=1; cd "$PKG_V2" && ./install.sh --prefix "$ROLL_FAIL_PREFIX" --rollback -y) 2>/dev/null || RET_FAIL_ROLL=$?
assert_eq "96" "$RET_FAIL_ROLL" "Fallo inyectado durante rollback capturado correctamente"

# Verificamos que rollback fallido se recuperó al estado previo
assert_eq "versions/$BUILD_ID_2" "$(readlink "$ROLL_FAIL_PREFIX/share/axialcad/current")" "current permanece en V2 tras rollback fallido"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$ROLL_FAIL_PREFIX/share/axialcad/previous")" "previous permanece en V1 tras rollback fallido"

# =============================================================================
# REQUISITO B2-8: Cancelación de migración deja instalación plana intacta
# =============================================================================
echo ""
echo "=== REQUISITO B2-8: Cancelación de migración deja instalación plana intacta ==="
CANCEL_PREFIX="$TEST_BASE/cancel_prefix"
mkdir -p "$CANCEL_PREFIX/bin"
mkdir -p "$CANCEL_PREFIX/share/cadcraft"
cat > "$CANCEL_PREFIX/bin/cadcraft" <<EOF
#!/usr/bin/env bash
echo "pre-existing flat binary"
EOF
chmod 755 "$CANCEL_PREFIX/bin/cadcraft"
cat > "$CANCEL_PREFIX/share/cadcraft/version.env" <<EOF
CADCRAFT_VERSION="0.0.8"
CADCRAFT_BUILD_SHA="1111111111111111111111111111111111111111"
EOF
cat > "$CANCEL_PREFIX/share/cadcraft/install-manifest.txt" <<EOF
bin/cadcraft
share/cadcraft/version.env
share/cadcraft/install-manifest.txt
EOF

# Simulamos que el usuario responde 'n' a la confirmación interactiva
echo "n" | (cd "$PKG_V1" && ./install.sh --prefix "$CANCEL_PREFIX") >/dev/null 2>&1 || true

assert_file_exists "$CANCEL_PREFIX/bin/cadcraft" "Binario plano original conservado tras cancelación"
assert_file_not_exists "$CANCEL_PREFIX/share/axialcad/current" "current no creado tras cancelación"
assert_file_not_exists "$CANCEL_PREFIX/share/axialcad/versions" "versions no creado tras cancelación"

# =============================================================================
# REQUISITO B2-9: Enlace de previous fuera del árbol es rechazado por rollback
# =============================================================================
echo ""
echo "=== REQUISITO B2-9: Enlace de previous fuera del árbol rechazado por rollback ==="
EVIL_PREV_PREFIX="$TEST_BASE/evil_prev_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$EVIL_PREV_PREFIX" -y) >/dev/null

# Forzamos previous a apuntar fuera del árbol de versions
OUTSIDE_TARGET="$TEST_BASE/outside_escape_target"
mkdir -p "$OUTSIDE_TARGET/bin"
cp "$PKG_V1/bin/axialcad" "$OUTSIDE_TARGET/bin/axialcad"
ln -sfn "$OUTSIDE_TARGET" "$EVIL_PREV_PREFIX/share/axialcad/previous"

RET_EVIL_PREV=0
(cd "$PKG_V1" && ./install.sh --prefix "$EVIL_PREV_PREFIX" --rollback -y) 2>/dev/null || RET_EVIL_PREV=$?
assert_eq "1" "$RET_EVIL_PREV" "Rollback aborta si previous apunta fuera del árbol versions/"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$EVIL_PREV_PREFIX/share/axialcad/current")" "current no conmutó ante previous malicioso"

# =============================================================================
# REQUISITO B2-10: Directorio padre enlazado fuera del prefijo es rechazado
# =============================================================================
echo ""
echo "=== REQUISITO B2-10: Directorio padre enlazado fuera del prefijo rechazado ==="
EVIL_PARENT_PREFIX="$TEST_BASE/evil_parent_prefix"
mkdir -p "$EVIL_PARENT_PREFIX/share"
OUTSIDE_DIR="$TEST_BASE/outside_dir"
mkdir -p "$OUTSIDE_DIR"
# Enlazamos bin a un directorio fuera del prefijo
ln -s "$OUTSIDE_DIR" "$EVIL_PARENT_PREFIX/bin"

RET_EVIL_PARENT=0
(cd "$PKG_V1" && ./install.sh --prefix "$EVIL_PARENT_PREFIX" -y) 2>/dev/null || RET_EVIL_PARENT=$?
assert_eq "1" "$RET_EVIL_PARENT" "El instalador rechaza directorios padres enlazados fuera del prefijo"

# =============================================================================
# REQUISITO B2-11: Archivo ajeno dentro de un directorio de versión es preservado
# =============================================================================
echo ""
echo "=== REQUISITO B2-11: Archivo ajeno dentro de una versión es preservado en desinstalación ==="
FOREIGN_VER_PREFIX="$TEST_BASE/foreign_ver_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$FOREIGN_VER_PREFIX" -y) >/dev/null

# Añadimos archivo ajeno dentro del directorio de la versión
USER_NOTE="$FOREIGN_VER_PREFIX/share/axialcad/versions/$BUILD_ID_1/user_custom_notes.txt"
echo "notas y scripts personalizados del usuario" > "$USER_NOTE"

# Desinstalamos
"$FOREIGN_VER_PREFIX/share/axialcad/uninstall.sh" --prefix "$FOREIGN_VER_PREFIX" -y >/dev/null

assert_file_exists "$USER_NOTE" "El archivo ajeno dentro del directorio de versión fue preservado"
assert_file_not_exists "$FOREIGN_VER_PREFIX/bin/axialcad" "Binario gestionado axialcad fue desinstalado"
assert_file_not_exists "$FOREIGN_VER_PREFIX/bin/cadcraft" "Alias cadcraft fue desinstalado"
assert_file_not_exists "$FOREIGN_VER_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad" "Binario de versión fue retirado"

# =============================================================================
# REQUISITO B2-12: Instalación de V1, V2 y V3 y posterior desinstalación
# (retira archivos gestionados de las 3 versiones y conserva archivos ajenos)
# =============================================================================
echo ""
echo "=== REQUISITO B2-12: Instalación V1, V2, V3 y desinstalación con preservación ajena ==="
MULTI_PREFIX="$TEST_BASE/multi_v123_prefix"
(cd "$PKG_V1" && ./install.sh --prefix "$MULTI_PREFIX" -y) >/dev/null
(cd "$PKG_V2" && ./install.sh --prefix "$MULTI_PREFIX" --update -y) >/dev/null
(cd "$PKG_V3" && ./install.sh --prefix "$MULTI_PREFIX" --update -y) >/dev/null

assert_file_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad" "V1 presente en árbol"
assert_file_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_2/bin/axialcad" "V2 presente en árbol"
assert_file_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_3/bin/axialcad" "V3 presente en árbol"

# Añadimos archivos ajenos dentro de V1, V2 y en la raíz de versions
echo "ajeno en V1" > "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_1/foreign_v1.txt"
echo "ajeno en V2" > "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_2/foreign_v2.txt"

# Desinstalamos
"$MULTI_PREFIX/share/axialcad/uninstall.sh" --prefix "$MULTI_PREFIX" -y >/dev/null

# Comprobamos que todos los archivos gestionados de V1, V2 y V3 fueron retirados
assert_file_not_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_1/bin/axialcad" "V1 binario retirado"
assert_file_not_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_2/bin/axialcad" "V2 binario retirado"
assert_file_not_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_3/bin/axialcad" "V3 binario retirado"
assert_file_not_exists "$MULTI_PREFIX/bin/axialcad" "Lanzador bin/axialcad retirado"
assert_file_not_exists "$MULTI_PREFIX/bin/cadcraft" "Alias bin/cadcraft retirado"
assert_file_not_exists "$MULTI_PREFIX/bin/axialcad-cli" "Lanzador bin/axialcad-cli retirado"
assert_file_not_exists "$MULTI_PREFIX/bin/cadcraft-cli" "Alias bin/cadcraft-cli retirado"
assert_file_not_exists "$MULTI_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" "Lanzador desktop retirado"

# Comprobamos que los archivos ajenos se preservaron
assert_file_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_1/foreign_v1.txt" "foreign_v1.txt preservado"
assert_file_exists "$MULTI_PREFIX/share/axialcad/versions/$BUILD_ID_2/foreign_v2.txt" "foreign_v2.txt preservado"

# =============================================================================
# PRUEBA OBLIGATORIA A: Primera instalación que falla después de crear recursos
# compartidos: no quedan lanzadores, iconos ni scripts nuevos de esa operación
# =============================================================================
echo ""
echo "=== PRUEBA OBLIGATORIA A: Fallo tras crear recursos compartidos retira archivos nuevos ==="
FAIL_A_PREFIX="$TEST_BASE/fail_a_prefix"
RET_FAIL_A=0
(export CADCRAFT_TEST_FAIL_AFTER_SHARED_RESOURCES=1; cd "$PKG_V1" && ./install.sh --prefix "$FAIL_A_PREFIX" -y) >/dev/null 2>&1 || RET_FAIL_A=$?
assert_eq "95" "$RET_FAIL_A" "Instalación abortada con código esperado tras crear recursos compartidos"

# Verificamos que no quedan lanzadores, iconos, scripts ni binarios nuevos
assert_file_not_exists "$FAIL_A_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" "Lanzador .desktop retirado limpiamente"
assert_file_not_exists "$FAIL_A_PREFIX/share/icons/hicolor/48x48/apps/ai.storyteller.cadcraft.png" "Iconos retirados limpiamente"
assert_file_not_exists "$FAIL_A_PREFIX/share/mime/packages/ai.storyteller.cadcraft.xml" "MIME xml retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/metainfo/ai.storyteller.cadcraft.metainfo.xml" "Metainfo xml retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/doc/cadcraft" "Directorio doc retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/axialcad/install.sh" "Script install.sh retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/axialcad/uninstall.sh" "Script uninstall.sh retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/axialcad/version.env" "version.env retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/axialcad/current" "Symlink current retirado"
assert_file_not_exists "$FAIL_A_PREFIX/bin/axialcad" "Binario de entrada bin/axialcad retirado"
assert_file_not_exists "$FAIL_A_PREFIX/bin/axialcad-cli" "Binario de entrada bin/axialcad-cli retirado"
assert_file_not_exists "$FAIL_A_PREFIX/bin/cadcraft" "Binario de entrada bin/cadcraft retirado"
assert_file_not_exists "$FAIL_A_PREFIX/bin/cadcraft-cli" "Binario de entrada bin/cadcraft-cli retirado"
assert_file_not_exists "$FAIL_A_PREFIX/share/axialcad/versions/$BUILD_ID_1" "Directorio de versión retirado"

# =============================================================================
# PRUEBA OBLIGATORIA B: Error simulado de cp durante recuperación:
# se conserva .recovery.*, se informa de recuperación incompleta y no se anuncia
# restauración correcta
# =============================================================================
echo ""
echo "=== PRUEBA OBLIGATORIA B: Error de cp durante recuperación conserva respaldo y avisa ==="
FAIL_B_PREFIX="$TEST_BASE/fail_b_prefix"
# Instalamos V1 normalmente primero
(cd "$PKG_V1" && ./install.sh --prefix "$FAIL_B_PREFIX" -y) >/dev/null

# Intentamos actualizar a V2 inyectando fallo tras current y simulación de fallo de cp en recuperación
OUTPUT_B=""
RET_FAIL_B=0
OUTPUT_B="$( (export CADCRAFT_TEST_FAIL_AFTER_CURRENT=1 CADCRAFT_TEST_MOCK_CP_FAIL=1; cd "$PKG_V2" && ./install.sh --prefix "$FAIL_B_PREFIX" --update -y) 2>&1 )" || RET_FAIL_B=$?

assert_eq "1" "$RET_FAIL_B" "Recuperación con fallo de copia termina con código de error 1"

# Comprobar que se informa de recuperación incompleta / errores de recuperación
echo "$OUTPUT_B" | grep -q "FATAL: recovery encountered errors" && INFORM_INCOMPLETE=1 || INFORM_INCOMPLETE=0
assert_eq "1" "$INFORM_INCOMPLETE" "Mensaje de recuperación incompleta/errores emitido en stderr"

# Comprobar que NO se anuncia restauración limpia
echo "$OUTPUT_B" | grep -q "system restored cleanly" && CLEAN_ANNOUNCED=1 || CLEAN_ANNOUNCED=0
assert_eq "0" "$CLEAN_ANNOUNCED" "NO se anuncia restauración limpia ('system restored cleanly') al fallar la copia"

# Comprobar que se conserva el directorio .recovery.*
RECOVERY_DIRS_COUNT="$(find "$FAIL_B_PREFIX/share/axialcad" -maxdepth 1 -name ".recovery.*" -type d | wc -l)"
[[ "$RECOVERY_DIRS_COUNT" -ge 1 ]] && HAS_REC_DIR=1 || HAS_REC_DIR=0
assert_eq "1" "$HAS_REC_DIR" "Directorio de respaldo .recovery.* conservado intacto para auditoría/auxilio"

# =============================================================================
# PRUEBA OBLIGATORIA C: Migración legacy que falla DESPUÉS de sustituir
# bin/cadcraft y bin/cadcraft-cli por enlaces:
# ambos vuelven a ser archivos regulares con contenido y permisos originales;
# manifiesto y metadatos vuelven al estado previo
# =============================================================================
echo ""
echo "=== PRUEBA OBLIGATORIA C: Fallo tras enlazar binarios en migración restaura archivos regulares ==="
FAIL_C_PREFIX="$TEST_BASE/fail_c_prefix"
mkdir -p "$FAIL_C_PREFIX/bin"
mkdir -p "$FAIL_C_PREFIX/share/cadcraft"
mkdir -p "$FAIL_C_PREFIX/share/applications"

# Crear instalación plana previa con bin/cadcraft (permisos 755) y bin/cadcraft-cli (permisos 750)
cat > "$FAIL_C_PREFIX/bin/cadcraft" <<EOF
#!/usr/bin/env bash
echo "legacy cadcraft content"
EOF
chmod 755 "$FAIL_C_PREFIX/bin/cadcraft"

cat > "$FAIL_C_PREFIX/bin/cadcraft-cli" <<EOF
#!/usr/bin/env bash
echo "legacy cadcraft-cli content"
EOF
chmod 750 "$FAIL_C_PREFIX/bin/cadcraft-cli"

cat > "$FAIL_C_PREFIX/share/cadcraft/version.env" <<EOF
CADCRAFT_VERSION="0.0.7"
CADCRAFT_BUILD_SHA="7777777777777777777777777777777777777777"
EOF

cat > "$FAIL_C_PREFIX/share/cadcraft/install-manifest.txt" <<EOF
bin/cadcraft
bin/cadcraft-cli
share/cadcraft/version.env
share/cadcraft/install-manifest.txt
EOF

# Guardamos hashes y permisos originales
HASH_ORIG_CAD="$(sha256sum "$FAIL_C_PREFIX/bin/cadcraft" | cut -d' ' -f1)"
HASH_ORIG_CLI="$(sha256sum "$FAIL_C_PREFIX/bin/cadcraft-cli" | cut -d' ' -f1)"
PERM_ORIG_CAD="$(stat -c "%a" "$FAIL_C_PREFIX/bin/cadcraft")"
PERM_ORIG_CLI="$(stat -c "%a" "$FAIL_C_PREFIX/bin/cadcraft-cli")"

# Ejecutamos instalación que intenta migrar y falla DESPUÉS de sustituir bin/cadcraft y bin/cadcraft-cli por enlaces
RET_FAIL_C=0
(export CADCRAFT_TEST_FAIL_AFTER_CURRENT=1; cd "$PKG_V1" && ./install.sh --prefix "$FAIL_C_PREFIX" -y) >/dev/null 2>&1 || RET_FAIL_C=$?
assert_eq "97" "$RET_FAIL_C" "Instalación con migración legacy abortó en punto de inyección tras enlaces"

# 1. Distinguir y comprobar que vuelven a ser archivos regulares, NO enlaces simbólicos
[[ -f "$FAIL_C_PREFIX/bin/cadcraft" && ! -L "$FAIL_C_PREFIX/bin/cadcraft" ]] && CAD_IS_REG=1 || CAD_IS_REG=0
[[ -f "$FAIL_C_PREFIX/bin/cadcraft-cli" && ! -L "$FAIL_C_PREFIX/bin/cadcraft-cli" ]] && CLI_IS_REG=1 || CLI_IS_REG=0
assert_eq "1" "$CAD_IS_REG" "bin/cadcraft restaurado como archivo regular (no enlace simbólico)"
assert_eq "1" "$CLI_IS_REG" "bin/cadcraft-cli restaurado como archivo regular (no enlace simbólico)"

# 2. Comprobar contenido original conservado idéntico
HASH_REST_CAD="$(sha256sum "$FAIL_C_PREFIX/bin/cadcraft" | cut -d' ' -f1)"
HASH_REST_CLI="$(sha256sum "$FAIL_C_PREFIX/bin/cadcraft-cli" | cut -d' ' -f1)"
assert_eq "$HASH_ORIG_CAD" "$HASH_REST_CAD" "Contenido original de bin/cadcraft restaurado intacto"
assert_eq "$HASH_ORIG_CLI" "$HASH_REST_CLI" "Contenido original de bin/cadcraft-cli restaurado intacto"

# 3. Comprobar permisos originales conservados (755 y 750)
PERM_REST_CAD="$(stat -c "%a" "$FAIL_C_PREFIX/bin/cadcraft")"
PERM_REST_CLI="$(stat -c "%a" "$FAIL_C_PREFIX/bin/cadcraft-cli")"
assert_eq "$PERM_ORIG_CAD" "$PERM_REST_CAD" "Permisos de bin/cadcraft conservados intactos ($PERM_ORIG_CAD)"
assert_eq "$PERM_ORIG_CLI" "$PERM_REST_CLI" "Permisos de bin/cadcraft-cli conservados intactos ($PERM_ORIG_CLI)"

# 4. Comprobar manifiesto y version.env devueltos al estado previo plano
grep -q "CADCRAFT_VERSION=\"0.0.7\"" "$FAIL_C_PREFIX/share/cadcraft/version.env" && META_REST=1 || META_REST=0
assert_eq "1" "$META_REST" "version.env devuelto al estado plano original (0.0.7)"
grep -q "bin/cadcraft-cli" "$FAIL_C_PREFIX/share/cadcraft/install-manifest.txt" && MANI_REST=1 || MANI_REST=0
assert_eq "1" "$MANI_REST" "install-manifest.txt devuelto al estado plano original"
assert_file_not_exists "$FAIL_C_PREFIX/share/axialcad/current" "Enlace current en axialcad no existe tras abortar migración fallida"
assert_file_not_exists "$FAIL_C_PREFIX/share/axialcad/versions" "Directorio versions en axialcad no existe tras abortar migración fallida"
assert_file_not_exists "$FAIL_C_PREFIX/share/axialcad" "Directorio share/axialcad no residual tras fallo de migración"
assert_file_not_exists "$FAIL_C_PREFIX/bin/axialcad" "Lanzador bin/axialcad no existe tras abortar migración fallida"

# =============================================================================
# REQUISITO PROPIEDAD Y COLISIONES: Detección estricta sin heurísticas
# =============================================================================
echo ""
echo "=== PROPIEDAD Y COLISIONES 1: .desktop preexistente no registrado aborta y conserva ==="
COLL_DESK_PREFIX="$TEST_BASE/coll_desk_prefix"
mkdir -p "$COLL_DESK_PREFIX/share/applications"
FOREIGN_DESK="$COLL_DESK_PREFIX/share/applications/ai.storyteller.cadcraft.desktop"
echo "custom foreign desktop launcher content" > "$FOREIGN_DESK"

RET_COLL_DESK=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_DESK_PREFIX" -y) 2>/dev/null || RET_COLL_DESK=$?
assert_eq "1" "$RET_COLL_DESK" "Instalador aborta ante .desktop preexistente no registrado"
assert_eq "custom foreign desktop launcher content" "$(cat "$FOREIGN_DESK")" "Contenido del .desktop ajeno conservado intacto"

echo ""
echo "=== PROPIEDAD Y COLISIONES 2: Icono preexistente no registrado aborta y conserva ==="
COLL_ICON_PREFIX="$TEST_BASE/coll_icon_prefix"
mkdir -p "$COLL_ICON_PREFIX/share/icons/hicolor/48x48/apps"
FOREIGN_ICON="$COLL_ICON_PREFIX/share/icons/hicolor/48x48/apps/ai.storyteller.cadcraft.png"
echo "custom foreign icon png bytes" > "$FOREIGN_ICON"

RET_COLL_ICON=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_ICON_PREFIX" -y) 2>/dev/null || RET_COLL_ICON=$?
assert_eq "1" "$RET_COLL_ICON" "Instalador aborta ante icono preexistente no registrado"
assert_eq "custom foreign icon png bytes" "$(cat "$FOREIGN_ICON")" "Contenido del icono ajeno conservado intacto"

echo ""
echo "=== PROPIEDAD Y COLISIONES 3: share/axialcad/install.sh preexistente no registrado aborta y conserva ==="
COLL_INST_PREFIX="$TEST_BASE/coll_inst_prefix"
mkdir -p "$COLL_INST_PREFIX/share/axialcad"
FOREIGN_INST="$COLL_INST_PREFIX/share/axialcad/install.sh"
echo "foreign installer script" > "$FOREIGN_INST"

RET_COLL_INST=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_INST_PREFIX" -y) 2>/dev/null || RET_COLL_INST=$?
assert_eq "1" "$RET_COLL_INST" "Instalador aborta ante share/axialcad/install.sh preexistente no registrado"
assert_eq "foreign installer script" "$(cat "$FOREIGN_INST")" "Contenido del install.sh ajeno conservado intacto"

echo ""
echo "=== PROPIEDAD Y COLISIONES 4: Enlace que parece propio pero resuelve fuera aborta sin modificar destino externo ==="
COLL_OUT_PREFIX="$TEST_BASE/coll_out_prefix"
mkdir -p "$COLL_OUT_PREFIX/bin"
OUTSIDE_VICTIM="$TEST_BASE/outside_sensitive_file.txt"
echo "important external file content" > "$OUTSIDE_VICTIM"
# Enlazamos bin/axialcad a un archivo fuera del prefijo
ln -s "$OUTSIDE_VICTIM" "$COLL_OUT_PREFIX/bin/axialcad"

RET_COLL_OUT=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_OUT_PREFIX" --force -y) 2>/dev/null || RET_COLL_OUT=$?
assert_eq "1" "$RET_COLL_OUT" "Instalador aborta ante enlace que resuelve fuera del prefijo, incluso con --force"
assert_eq "important external file content" "$(cat "$OUTSIDE_VICTIM")" "Destino externo fuera del prefijo NO modificado"

echo ""
echo "=== PROPIEDAD Y COLISIONES 5: Instalación y actualización legítimas con manifiesto funcionan ==="
LEGIT_PREFIX="$TEST_BASE/legit_manifest_prefix"
# 1. Instalación inicial limpia
RET_LEGIT_INST=0
(cd "$PKG_V1" && ./install.sh --prefix "$LEGIT_PREFIX" -y) >/dev/null 2>&1 || RET_LEGIT_INST=$?
assert_eq "0" "$RET_LEGIT_INST" "Instalación legítima inicial en prefijo limpio tiene éxito"
assert_file_exists "$LEGIT_PREFIX/share/axialcad/install-manifest.txt" "Manifiesto creado en instalación legítima"

# 2. Actualización sobre instalación existente con manifiesto
RET_LEGIT_UPD=0
(cd "$PKG_V2" && ./install.sh --prefix "$LEGIT_PREFIX" --update -y) >/dev/null 2>&1 || RET_LEGIT_UPD=$?
assert_eq "0" "$RET_LEGIT_UPD" "Actualización legítima sobre instalación con manifiesto tiene éxito sin falsas colisiones"
assert_eq "versions/$BUILD_ID_2" "$(readlink "$LEGIT_PREFIX/share/axialcad/current")" "current conmuta legítimamente a V2"

echo ""
echo "=== PROPIEDAD Y COLISIONES 6: Enlace roto de destino aborta incluso con --force y conserva enlace ==="
COLL_BROKEN_PREFIX="$TEST_BASE/coll_broken_prefix"
mkdir -p "$COLL_BROKEN_PREFIX/bin"
BROKEN_LINK="$COLL_BROKEN_PREFIX/bin/axialcad"
ln -s "$TEST_BASE/nonexistent_broken_target" "$BROKEN_LINK"

RET_COLL_BROKEN=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_BROKEN_PREFIX" --force -y) 2>/dev/null || RET_COLL_BROKEN=$?
assert_eq "1" "$RET_COLL_BROKEN" "Instalador aborta ante enlace roto de destino incluso con --force"
[[ -L "$BROKEN_LINK" ]] && STILL_BROKEN_LINK=1 || STILL_BROKEN_LINK=0
assert_eq "1" "$STILL_BROKEN_LINK" "Enlace roto de destino conservado intacto sin ser sobrescrito"
assert_eq "$TEST_BASE/nonexistent_broken_target" "$(readlink "$BROKEN_LINK")" "Destino del enlace roto conservado intacto"

echo ""
echo "=== PROPIEDAD Y COLISIONES 7: Bucle de enlaces de destino aborta incluso con --force y conserva enlace ==="
COLL_LOOP_PREFIX="$TEST_BASE/coll_loop_prefix"
mkdir -p "$COLL_LOOP_PREFIX/bin"
LOOP_LINK="$COLL_LOOP_PREFIX/bin/axialcad"
ln -s "$LOOP_LINK" "$LOOP_LINK"

RET_COLL_LOOP=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_LOOP_PREFIX" --force -y) 2>/dev/null || RET_COLL_LOOP=$?
assert_eq "1" "$RET_COLL_LOOP" "Instalador aborta ante bucle de enlaces de destino incluso con --force"
[[ -L "$LOOP_LINK" ]] && STILL_LOOP_LINK=1 || STILL_LOOP_LINK=0
assert_eq "1" "$STILL_LOOP_LINK" "Bucle de enlaces conservado intacto sin ser sobrescrito"

echo ""
echo "=== PROPIEDAD Y COLISIONES 8: Entrada de manifiesto cuyo padre no puede resolverse aborta y conserva estado ==="
COLL_BAD_PARENT_PREFIX="$TEST_BASE/coll_bad_parent_prefix"
mkdir -p "$COLL_BAD_PARENT_PREFIX/share/axialcad"
CANARY_FILE="$COLL_BAD_PARENT_PREFIX/canary.txt"
echo "canary before bad manifest" > "$CANARY_FILE"
cat > "$COLL_BAD_PARENT_PREFIX/share/axialcad/install-manifest.txt" <<EOF
nonexistent_parent_dir/sub/file.txt
EOF

RET_COLL_BAD_PARENT=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_BAD_PARENT_PREFIX" --update -y) 2>/dev/null || RET_COLL_BAD_PARENT=$?
assert_eq "1" "$RET_COLL_BAD_PARENT" "Instalador aborta ante entrada de manifiesto con padre no resoluble"
assert_file_exists "$CANARY_FILE" "Canario en prefijo conservado"
assert_eq "canary before bad manifest" "$(cat "$CANARY_FILE")" "Estado previo del prefijo conservado intacto"

echo ""
echo "=== PROPIEDAD Y COLISIONES 9: Archivo gestionado ausente con padre válido permite actualización legítima ==="
COLL_ABSENT_PREFIX="$TEST_BASE/coll_absent_prefix"
# 1. Instalar V1
RET_ABSENT_INST=0
(cd "$PKG_V1" && ./install.sh --prefix "$COLL_ABSENT_PREFIX" -y) >/dev/null 2>&1 || RET_ABSENT_INST=$?
assert_eq "0" "$RET_ABSENT_INST" "Instalación V1 en prefijo para prueba de archivo ausente tiene éxito"

# 2. Borrar un archivo acreditado en el manifiesto pero cuyo directorio padre sigue existiendo y contenido
ABSENT_FILE="$COLL_ABSENT_PREFIX/share/mime/packages/ai.storyteller.cadcraft.xml"
assert_file_exists "$ABSENT_FILE" "Archivo MIME existe tras V1"
rm -f "$ABSENT_FILE"
assert_file_not_exists "$ABSENT_FILE" "Archivo MIME eliminado para simular archivo gestionado ausente"
assert_file_exists "$COLL_ABSENT_PREFIX/share/mime/packages" "Directorio padre existe y es verificable"

# =============================================================================
# ENTREGA B: Migración desde share/cadcraft versionado, rollback y compatibilidad
# =============================================================================
echo ""
echo "=== ENTREGA B: Migración desde share/cadcraft versionado a share/axialcad ==="
ENTREGA_B_PREFIX="$TEST_BASE/entrega_b_mig_prefix"
SHA_LEGACY="9999999999999999999999999999999999999999"
LEGACY_VER_DIR="$ENTREGA_B_PREFIX/share/cadcraft/versions/0.0.9-$SHA_LEGACY"
mkdir -p "$LEGACY_VER_DIR/bin"
mkdir -p "$ENTREGA_B_PREFIX/bin"
mkdir -p "$ENTREGA_B_PREFIX/share/applications"
mkdir -p "$ENTREGA_B_PREFIX/share/icons/hicolor/48x48/apps"
mkdir -p "$ENTREGA_B_PREFIX/share/mime/packages"

# Crear binario ejecutable legacy cadcraft
cat > "$LEGACY_VER_DIR/bin/cadcraft" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "cadcraft 0.0.9 ($SHA_LEGACY)"; exit 0; fi
echo "legacy_cadcraft_output"
EOF
chmod 755 "$LEGACY_VER_DIR/bin/cadcraft"

# Crear binario ejecutable legacy cadcraft-cli
cat > "$LEGACY_VER_DIR/bin/cadcraft-cli" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "cadcraft-cli 0.0.9 ($SHA_LEGACY)"; exit 0; fi
echo "legacy_cli_output"
EOF
chmod 755 "$LEGACY_VER_DIR/bin/cadcraft-cli"

# Enlace simbólico current legacy
ln -s "versions/0.0.9-$SHA_LEGACY" "$ENTREGA_B_PREFIX/share/cadcraft/current"

# Metadatos legacy
cat > "$ENTREGA_B_PREFIX/share/cadcraft/version.env" <<EOF
CADCRAFT_VERSION="0.0.9"
CADCRAFT_BUILD_SHA="$SHA_LEGACY"
CADCRAFT_BUILD_DATE="2026-10-01T12:00:00Z"
CADCRAFT_ARCH="x86_64"
EOF

# Manifiesto legacy
cat > "$ENTREGA_B_PREFIX/share/cadcraft/install-manifest.txt" <<EOF
share/cadcraft/install.sh
share/cadcraft/uninstall.sh
share/cadcraft/version.env
share/cadcraft/install-manifest.txt
share/applications/ai.storyteller.cadcraft.desktop
share/mime/packages/ai.storyteller.cadcraft.xml
bin/cadcraft
bin/cadcraft-cli
EOF

# Scripts legacy en share/cadcraft
cp "$INSTALL_SRC" "$ENTREGA_B_PREFIX/share/cadcraft/install.sh"
cp "$UNINSTALL_SRC" "$ENTREGA_B_PREFIX/share/cadcraft/uninstall.sh"

# Enlaces en bin legacy
ln -s "../share/cadcraft/current/bin/cadcraft" "$ENTREGA_B_PREFIX/bin/cadcraft"
ln -s "../share/cadcraft/current/bin/cadcraft-cli" "$ENTREGA_B_PREFIX/bin/cadcraft-cli"

# Archivo de usuario no gestionado dentro de share/cadcraft (debe ser preservado)
echo "user custom configuration" > "$ENTREGA_B_PREFIX/share/cadcraft/user_custom.conf"

# Ejecutar instalador nuevo en modo actualización
RET_ENTREGA_B=0
(cd "$PKG_V1" && ./install.sh --prefix "$ENTREGA_B_PREFIX" --update -y) >/dev/null 2>&1 || RET_ENTREGA_B=$?
assert_eq "0" "$RET_ENTREGA_B" "Migración desde share/cadcraft a share/axialcad completada con éxito"

# 1. Comprobar que share/axialcad es la nueva raíz activa
assert_file_exists "$ENTREGA_B_PREFIX/share/axialcad/current" "share/axialcad/current existe"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$ENTREGA_B_PREFIX/share/axialcad/current")" "current en axialcad apunta a V1"
assert_eq "versions/0.0.9-$SHA_LEGACY" "$(readlink "$ENTREGA_B_PREFIX/share/axialcad/previous")" "previous apunta a la versión legacy migrada"

# 2. Comprobar enlaces en bin
assert_eq "../share/axialcad/current/bin/axialcad" "$(readlink "$ENTREGA_B_PREFIX/bin/axialcad")" "bin/axialcad apunta a current"
assert_eq "axialcad" "$(readlink "$ENTREGA_B_PREFIX/bin/cadcraft")" "bin/cadcraft apunta a axialcad (alias compatibilidad)"
assert_eq "../share/axialcad/current/bin/axialcad-cli" "$(readlink "$ENTREGA_B_PREFIX/bin/axialcad-cli")" "bin/axialcad-cli apunta a current"
assert_eq "axialcad-cli" "$(readlink "$ENTREGA_B_PREFIX/bin/cadcraft-cli")" "bin/cadcraft-cli apunta a axialcad-cli (alias compatibilidad)"

# 3. Comprobar invocaciones públicas y de compatibilidad
assert_eq "v1_content" "$("$ENTREGA_B_PREFIX/bin/axialcad")" "Invocación directa axialcad ejecuta nueva versión"
assert_eq "v1_content" "$("$ENTREGA_B_PREFIX/bin/cadcraft")" "Invocación compatible cadcraft ejecuta nueva versión"
assert_eq "v1_content" "$("$ENTREGA_B_PREFIX/bin/axialcad-cli")" "Invocación directa axialcad-cli ejecuta nueva versión"
assert_eq "v1_content" "$("$ENTREGA_B_PREFIX/bin/cadcraft-cli")" "Invocación compatible cadcraft-cli ejecuta nueva versión"

# 4. Comprobar lanzador desktop
grep -q 'Exec=".*bin/axialcad" %F' "$ENTREGA_B_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" && B_DESK_EXEC=1 || B_DESK_EXEC=0
assert_eq "1" "$B_DESK_EXEC" "Lanzador desktop actualizado con Exec=axialcad"
grep -q "TryExec=.*bin/axialcad" "$ENTREGA_B_PREFIX/share/applications/ai.storyteller.cadcraft.desktop" && B_DESK_TRY=1 || B_DESK_TRY=0
assert_eq "1" "$B_DESK_TRY" "Lanzador desktop actualizado con TryExec=axialcad"

# 5. Comprobar preservación del archivo ajeno en share/cadcraft
assert_file_exists "$ENTREGA_B_PREFIX/share/cadcraft/user_custom.conf" "Archivo ajeno en share/cadcraft preservado"
assert_eq "user custom configuration" "$(cat "$ENTREGA_B_PREFIX/share/cadcraft/user_custom.conf")" "Contenido ajeno en share/cadcraft intacto"

# 6. Comprobar rollback a la versión legacy migrada
echo ""
echo "=== ENTREGA B: Rollback a la versión legacy migrada ==="
RET_B_ROLL=0
(cd "$PKG_V1" && "$ENTREGA_B_PREFIX/share/axialcad/install.sh" --prefix "$ENTREGA_B_PREFIX" --rollback -y) >/dev/null 2>&1 || RET_B_ROLL=$?
assert_eq "0" "$RET_B_ROLL" "Rollback a versión legacy migrada completado con éxito"
assert_eq "versions/0.0.9-$SHA_LEGACY" "$(readlink "$ENTREGA_B_PREFIX/share/axialcad/current")" "current conmuta a versión legacy tras rollback"
assert_eq "versions/$BUILD_ID_1" "$(readlink "$ENTREGA_B_PREFIX/share/axialcad/previous")" "previous guarda V1 tras rollback"

# Invocaciones tras rollback deben ejecutar el binario legacy
assert_eq "legacy_cadcraft_output" "$("$ENTREGA_B_PREFIX/bin/axialcad")" "axialcad ejecuta binario legacy tras rollback"
assert_eq "legacy_cadcraft_output" "$("$ENTREGA_B_PREFIX/bin/cadcraft")" "cadcraft ejecuta binario legacy tras rollback"
assert_eq "legacy_cli_output" "$("$ENTREGA_B_PREFIX/bin/axialcad-cli")" "axialcad-cli ejecuta binario legacy tras rollback"
assert_eq "legacy_cli_output" "$("$ENTREGA_B_PREFIX/bin/cadcraft-cli")" "cadcraft-cli ejecuta binario legacy tras rollback"

# 7. Comprobar desinstalación completa
echo ""
echo "=== ENTREGA B: Desinstalación limpia tras migración ==="
RET_B_UNINST=0
"$ENTREGA_B_PREFIX/share/axialcad/uninstall.sh" --prefix "$ENTREGA_B_PREFIX" -y >/dev/null 2>&1 || RET_B_UNINST=$?
assert_eq "0" "$RET_B_UNINST" "Desinstalador ejecutado con éxito"
assert_file_not_exists "$ENTREGA_B_PREFIX/bin/axialcad" "bin/axialcad retirado tras desinstalación"
assert_file_not_exists "$ENTREGA_B_PREFIX/bin/cadcraft" "bin/cadcraft retirado tras desinstalación"
assert_file_not_exists "$ENTREGA_B_PREFIX/bin/axialcad-cli" "bin/axialcad-cli retirado tras desinstalación"
assert_file_not_exists "$ENTREGA_B_PREFIX/bin/cadcraft-cli" "bin/cadcraft-cli retirado tras desinstalación"
assert_file_not_exists "$ENTREGA_B_PREFIX/share/axialcad" "share/axialcad retirado tras desinstalación"
# Archivo de usuario en share/cadcraft sigue existiendo
assert_file_exists "$ENTREGA_B_PREFIX/share/cadcraft/user_custom.conf" "Archivo de usuario preservado tras desinstalación"

# 8. Migración sin archivos ajenos poda limpiamente share/cadcraft
echo ""
echo "=== ENTREGA B: Migración sin archivos ajenos poda share/cadcraft vacío ==="
CLEAN_MIG_PREFIX="$TEST_BASE/clean_mig_prefix"
mkdir -p "$CLEAN_MIG_PREFIX/share/cadcraft/versions/0.0.9-$SHA_LEGACY/bin"
mkdir -p "$CLEAN_MIG_PREFIX/bin"
mkdir -p "$CLEAN_MIG_PREFIX/share/applications"
cat > "$CLEAN_MIG_PREFIX/share/cadcraft/versions/0.0.9-$SHA_LEGACY/bin/cadcraft" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then echo "cadcraft 0.0.9 ($SHA_LEGACY)"; exit 0; fi
EOF
chmod 755 "$CLEAN_MIG_PREFIX/share/cadcraft/versions/0.0.9-$SHA_LEGACY/bin/cadcraft"
ln -s "versions/0.0.9-$SHA_LEGACY" "$CLEAN_MIG_PREFIX/share/cadcraft/current"
cat > "$CLEAN_MIG_PREFIX/share/cadcraft/version.env" <<EOF
CADCRAFT_VERSION="0.0.9"
CADCRAFT_BUILD_SHA="$SHA_LEGACY"
EOF
cat > "$CLEAN_MIG_PREFIX/share/cadcraft/install-manifest.txt" <<EOF
share/cadcraft/version.env
share/cadcraft/install-manifest.txt
bin/cadcraft
EOF
ln -s "../share/cadcraft/current/bin/cadcraft" "$CLEAN_MIG_PREFIX/bin/cadcraft"

(cd "$PKG_V1" && ./install.sh --prefix "$CLEAN_MIG_PREFIX" --update -y) >/dev/null 2>&1
assert_file_exists "$CLEAN_MIG_PREFIX/share/axialcad/current" "Nueva instalación activa en share/axialcad"
assert_file_not_exists "$CLEAN_MIG_PREFIX/share/cadcraft" "share/cadcraft vacío fue podado limpiamente tras migración"

# 9. Archivo regular ajeno y symlink ajeno en cualquiera de las cuatro entradas públicas: no se sobrescriben, ni con --force
echo ""
echo "=== ENTREGA B: Protección absoluta de las 4 entradas públicas ante archivos/symlinks ajenos (incluso --force) ==="
for entry_name in "axialcad" "cadcraft" "axialcad-cli" "cadcraft-cli"; do
  # A: Archivo regular ajeno
  REG_PREFIX="$TEST_BASE/prot_reg_${entry_name}"
  mkdir -p "$REG_PREFIX/bin"
  echo "external_binary_${entry_name}" > "$REG_PREFIX/bin/${entry_name}"
  chmod 755 "$REG_PREFIX/bin/${entry_name}"

  RET_REG_COLL=0
  (cd "$PKG_V1" && ./install.sh --prefix "$REG_PREFIX" --force -y) >/dev/null 2>&1 || RET_REG_COLL=$?
  assert_eq "1" "$RET_REG_COLL" "Instalador aborta ante archivo regular ajeno en bin/${entry_name} incluso con --force"
  assert_eq "external_binary_${entry_name}" "$(cat "$REG_PREFIX/bin/${entry_name}")" "Contenido de archivo ajeno en bin/${entry_name} intacto"

  # B: Symlink ajeno válido interno
  SYM_PREFIX="$TEST_BASE/prot_sym_${entry_name}"
  mkdir -p "$SYM_PREFIX/bin" "$SYM_PREFIX/custom"
  echo "custom_target_${entry_name}" > "$SYM_PREFIX/custom/target.txt"
  ln -s "../custom/target.txt" "$SYM_PREFIX/bin/${entry_name}"

  RET_SYM_COLL=0
  (cd "$PKG_V1" && ./install.sh --prefix "$SYM_PREFIX" --force -y) >/dev/null 2>&1 || RET_SYM_COLL=$?
  assert_eq "1" "$RET_SYM_COLL" "Instalador aborta ante symlink ajeno en bin/${entry_name} incluso con --force"
  assert_eq "../custom/target.txt" "$(readlink "$SYM_PREFIX/bin/${entry_name}")" "Symlink ajeno en bin/${entry_name} conservado intacto"
done

# 10. Manifiesto con ruta fuera del ámbito gestionado: no permite borrar archivos externos
echo ""
echo "=== ENTREGA B: Manifiesto con ruta fuera del ámbito no permite borrar archivos externos ==="
ESCAPE_MAN_PREFIX="$TEST_BASE/escape_man_prefix"
mkdir -p "$ESCAPE_MAN_PREFIX/share/axialcad"
CANARY_FILE="$TEST_BASE/external_canary_file.txt"
echo "canary_alive" > "$CANARY_FILE"
cat > "$ESCAPE_MAN_PREFIX/share/axialcad/install-manifest.txt" <<EOF
share/axialcad/install-manifest.txt
../../external_canary_file.txt
/tmp/nonexistent_or_system_file
EOF
touch "$ESCAPE_MAN_PREFIX/share/axialcad/uninstall.sh"
chmod 755 "$ESCAPE_MAN_PREFIX/share/axialcad/uninstall.sh"
cp "$UNINSTALL_SRC" "$ESCAPE_MAN_PREFIX/share/axialcad/uninstall.sh"

RET_ESC_UNINST=0
"$ESCAPE_MAN_PREFIX/share/axialcad/uninstall.sh" --prefix "$ESCAPE_MAN_PREFIX" -y >/dev/null 2>&1 || RET_ESC_UNINST=$?
assert_file_exists "$CANARY_FILE" "Archivo externo fuera del ámbito NO fue borrado por desinstalador con manifiesto malicioso"
assert_eq "canary_alive" "$(cat "$CANARY_FILE")" "Contenido del archivo externo permanece intacto"

# 11. Coexistencia ambigua de share/cadcraft y share/axialcad: aborta sin pérdida
echo ""
echo "=== ENTREGA B: Coexistencia ambigua de share/cadcraft y share/axialcad aborta sin pérdida ==="
AMBIG_PREFIX="$TEST_BASE/ambig_prefix"
mkdir -p "$AMBIG_PREFIX/share/axialcad/versions/0.1.0-axial/bin"
mkdir -p "$AMBIG_PREFIX/share/cadcraft/versions/0.0.9-cad/bin"
mkdir -p "$AMBIG_PREFIX/bin"
echo "axial_active" > "$AMBIG_PREFIX/share/axialcad/versions/0.1.0-axial/bin/axialcad"
echo "cad_active" > "$AMBIG_PREFIX/share/cadcraft/versions/0.0.9-cad/bin/cadcraft"
ln -s "versions/0.1.0-axial" "$AMBIG_PREFIX/share/axialcad/current"
ln -s "versions/0.0.9-cad" "$AMBIG_PREFIX/share/cadcraft/current"
touch "$AMBIG_PREFIX/share/axialcad/install-manifest.txt"
touch "$AMBIG_PREFIX/share/cadcraft/install-manifest.txt"

RET_AMBIG=0
(cd "$PKG_V2" && ./install.sh --prefix "$AMBIG_PREFIX" --update -y) >/dev/null 2>&1 || RET_AMBIG=$?
assert_eq "1" "$RET_AMBIG" "Instalador aborta ante coexistencia ambigua con dos instalaciones activas"
assert_eq "versions/0.1.0-axial" "$(readlink "$AMBIG_PREFIX/share/axialcad/current")" "Estado de share/axialcad conservado intacto"
assert_eq "versions/0.0.9-cad" "$(readlink "$AMBIG_PREFIX/share/cadcraft/current")" "Estado de share/cadcraft conservado intacto"

# 12. Prefijo con espacios y enlaces rotos
echo ""
echo "=== ENTREGA B: Prefijo con espacios y enlaces rotos ==="
SPACE_PREFIX="$TEST_BASE/prefix with spaces and symbols"
mkdir -p "$SPACE_PREFIX"
RET_SPACE_INST=0
(cd "$PKG_V1" && ./install.sh --prefix "$SPACE_PREFIX" -y) >/dev/null 2>&1 || RET_SPACE_INST=$?
assert_eq "0" "$RET_SPACE_INST" "Instalación en prefijo con espacios completada con éxito"
assert_eq "v1_content" "$("$SPACE_PREFIX/bin/axialcad")" "Invocación de binario en prefijo con espacios funciona"

SPACE_BROKEN_PREFIX="$TEST_BASE/prefix with spaces broken"
mkdir -p "$SPACE_BROKEN_PREFIX/bin"
ln -s "$TEST_BASE/nonexistent_target_link" "$SPACE_BROKEN_PREFIX/bin/axialcad"
RET_SPACE_BROKEN=0
(cd "$PKG_V1" && ./install.sh --prefix "$SPACE_BROKEN_PREFIX" --force -y) >/dev/null 2>&1 || RET_SPACE_BROKEN=$?
assert_eq "1" "$RET_SPACE_BROKEN" "Instalador aborta ante enlace roto dentro de prefijo con espacios incluso con --force"
assert_file_exists "$SPACE_BROKEN_PREFIX/bin/axialcad" "Enlace roto dentro de prefijo con espacios preservado"

echo ""
echo "================================================================="
echo "TEST SUITE BLOQUE 1 & BLOQUE 2 SUMMARY:"
echo "  Total passed checks: $PASSED_COUNT"
echo "  Total failed checks: $FAILED_COUNT"
echo "================================================================="
if [[ "$FAILED_COUNT" -gt 0 ]]; then
  exit 1
fi
exit 0
