# Distribución y Empaquetado Linux de CADcraft

Este documento describe la arquitectura de empaquetado, la estructura de artefactos generados, las dependencias del sistema, el ciclo de vida de configuración y las directrices para la validación y lanzamiento de paquetes.

---

## 1. Formatos de Entrega y Alcance de Compatibilidad

### Formatos Generados
En el estado actual de la plataforma, el formato principal verificado es el **Tarball de Usuario Autocontenido (`.tar.gz`)**, complementado con el formato alternativo **AppImage**:

| Formato | Estado de Validación | Dependencias del Sistema | Comportamiento y Despliegue |
|---|---|---|---|
| **Tarball de Usuario** (`.tar.gz`) — *Principal* | **Construido y verificado localmente** en CachyOS x86_64. Probado en instalación aislada, arranque gráfico, menú `.desktop`, apertura DWG/DXF, actualización y desinstalación. | Bibliotecas estándar C: `libc.so.6`, `libm.so.6`, `libgcc_s.so.1`, `ld-linux-x86-64.so.2`. Stack gráfico resuelto dinámicamente vía dlopen (Wayland / X11 / Vulkan / Mesa). | Contiene `install.sh` y `uninstall.sh`. Se instala por usuario sin root (por defecto en `~/.local/`). Configura la entrada `.desktop` con ruta absoluta para funcionar incluso si `~/.local/bin` no está en el `$PATH` gráfico. |
| **AppImage** (`.AppImage`) — *Alternativo* | **Construido y verificado localmente** en ejecución directa (`--version`) y arranque gráfico con canal de control (`ping` / `app.quit`). | Mismas dependencias de glibc/libm del binario. Runtime integrado tipo 2 (no requiere libfuse externo en el anfitrión). | Archivo ejecutable único portable. **No realiza integración automática en el menú de aplicaciones ni dock** salvo que el usuario emplee herramientas externas (como AppImageLauncher) o cree un lanzador manual. |

### Compatibilidad Real de Símbolos y Dependencias Dinámicas
La inspección de versiones de símbolos ELF mediante `readelf -V` y `readelf -s` arroja los siguientes requisitos exactos para los binarios construidos localmente en CachyOS:

- **Enlace de bibliotecas:** Exclusivamente dependencias estándar del sistema (`libc.so.6`, `libm.so.6`, `libgcc_s.so.1`, `ld-linux-x86-64.so.2`). No hay enlace estático ni dinámico contra bibliotecas C externas de interfaz gráfica (egui y wgpu resuelven Wayland/X11 y Vulkan en tiempo de ejecución).
- **Requisito en `libm.so.6`:**
  - `cadcraft`: Requiere **`GLIBC_2.44`** debido a `cosh@GLIBC_2.44` y `sinh@GLIBC_2.44` (además de `acosf@GLIBC_2.43`, `sinhf@GLIBC_2.43`, `coshf@GLIBC_2.43`, `asinf@GLIBC_2.43` y `hypot@GLIBC_2.35`).
  - `cadcraft-cli`: Requiere **`GLIBC_2.43`** debido a `acosf@GLIBC_2.43` y `hypot@GLIBC_2.35`.
- **Requisito en `libc.so.6`:**
  - El símbolo no débil más alto es `GLIBC_2.34` (funciones `pidfd_spawn` y `pidfd_spawnp` marcadas como `WEAK` en `GLIBC_2.39`).

#### Distribuciones Comprobadas y Estado Pendiente:
- **CachyOS Linux x86_64 (Sobremesa):** Probado y validado en entorno real nativo.
- **Fedora (Portátil):** **Pendiente de prueba.** No se puede garantizar compatibilidad universal con el binario compilado localmente en CachyOS si la versión de Fedora dispone de una versión de glibc inferior a 2.44.
- **Binarios de CI / GitHub Releases:** El workflow de publicación oficial compila sobre Ubuntu 22.04 LTS (glibc 2.35), produciendo binarios que sí son compatibles con Fedora y la gran mayoría de distribuciones Linux modernas.

---

## 2. Estructura del Paquete Tarball e Instalador

El archivo comprimido `cadcraft-<version>-linux-<arch>.tar.gz` contiene la siguiente estructura:

```text
cadcraft-<version>-linux-<arch>/
├── bin/
│   ├── cadcraft
│   └── cadcraft-cli
├── install.sh
├── uninstall.sh
├── version.env
├── README.txt
└── share/
    ├── applications/
    │   └── ai.storyteller.cadcraft.desktop
    ├── doc/
    │   └── cadcraft/
    │       ├── README.md
    │       ├── LICENSE-MIT
    │       └── LICENSE-APACHE
    ├── icons/
    │   └── hicolor/ (16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256, 512x512)
    ├── metainfo/
    │   └── ai.storyteller.cadcraft.metainfo.xml
    └── mime/
        └── packages/
            └── ai.storyteller.cadcraft.xml
```

### Seguridad y Robustez de los Scripts
- **Detección de colisiones:** `install.sh` comprueba si existen archivos previos en las rutas de destino. Si existen y no pertenecen a un manifiesto previo de CADCraft, la instalación se detiene informando del conflicto para no sobrescribir archivos ajenos (se puede forzar con `--force`).
- **Validación de rutas y prevención de escapes:** Tanto `install.sh` como `uninstall.sh` validan cada entrada del manifiesto, rechazando rutas absolutas o secuencias de escape (`..`) para garantizar que las operaciones se limiten estrictamente al prefijo configurado.
- **Rollback atómico:** Si ocurre un fallo durante la instalación o actualización, el instalador restaura la copia de seguridad previa de los ficheros y aborta sin reportar falso éxito.
- **Soporte de espacios y caracteres no ASCII:** Se admiten prefijos con espacios y tildes/UTF-8. La clave `Exec` del archivo `.desktop` se formatea con comillas dobles (`Exec="<prefix>/bin/cadcraft" %F`).

---

## 3. Procedimiento de Instalación, Actualización y Desinstalación

### A. Descarga e Instalación Inicial
> **Importante:** Descargar siempre el archivo de paquete `cadcraft-<version>-linux-x86_64.tar.gz` de la sección **Assets** de la Release en GitHub (no descargar el archivo de código fuente *Source code (zip/tar.gz)*).

```bash
# 1. Extraer el paquete descargado
tar -xzf cadcraft-*-linux-x86_64.tar.gz
cd cadcraft-*-linux-x86_64

# 2. Ejecutar el instalador (por defecto instala en ~/.local/)
./install.sh

# Opcional: especificar otro prefijo
./install.sh --prefix /opt/cadcraft
```

El instalador:
1. Copia los ejecutables a `<prefix>/bin/`.
2. Instala la entrada `.desktop` en `<prefix>/share/applications/` con ruta absoluta.
3. Copia la colección de iconos y definiciones MIME.
4. Escribe el manifiesto de instalación en `<prefix>/share/cadcraft/install-manifest.txt`.
5. Ejecuta `update-desktop-database`, `gtk-update-icon-cache` y `update-mime-database`.

### B. Actualización de Versión
Para actualizar una instalación existente conservando dibujos, plantillas y configuración:
```bash
tar -xzf cadcraft-<nueva-version>-linux-x86_64.tar.gz
cd cadcraft-<nueva-version>-linux-x86_64
./install.sh --update
```
- Sustituye únicamente los archivos gestionados por la aplicación.
- Si CADCraft está abierto, solicita cerrarlo antes de proceder.
- Mantiene intactos los archivos de configuración (`~/.config/cadcraft`), estado de sesión (`~/.local/share/ai.storyteller.cadcraft`), y dibujos.

### C. Desinstalación Limpia
```bash
~/.local/share/cadcraft/uninstall.sh
# O ejecutando ./uninstall.sh desde la carpeta del paquete
```
- Lee `install-manifest.txt` y borra únicamente los archivos instalados por CADCraft.
- No utiliza comodines que puedan eliminar archivos ajenos en directorios compartidos.
- Actualiza las bases de datos de escritorio y MIME para retirar el acceso del menú.
- Conserva intactos todos los documentos personales y configuraciones de usuario.

---

## 4. Conservación de Preferencias y Datos

CADCraft separa estrictamente los binarios de la configuración según los estándares XDG:
- **Configuración y logs:** `$XDG_CONFIG_HOME/cadcraft/` (por defecto `~/.config/cadcraft/`).
- **Almacén de estado y vistas:** `$XDG_DATA_HOME/ai.storyteller.cadcraft/` (por defecto `~/.local/share/ai.storyteller.cadcraft/app.ron`).
- **Documentos de usuario:** Guardados en las carpetas personales elegidas por el usuario, sin vincularse al directorio del programa.

---

## 5. Procedimiento de Publicación en GitHub Releases

1. **Compilación de paquetes y sumas de comprobación:**
   ```bash
   CARGO_TARGET_DIR=/ruta/target TMPDIR=/ruta/tmp DIST=/ruta/dist ./packaging/linux/package.sh --formats "tar appimage"
   ```
2. **Generación de artefactos:**
   - `cadcraft-<version>-linux-<arch>.tar.gz` + `.sha256`
   - `cadcraft-<version>-linux-<arch>.manifest.txt`
   - `cadcraft-<version>-linux-<arch>.AppImage` + `.sha256`
3. **Flujo de Publicación:**
   - Crear una etiqueta de versión explícita en el repositorio privado tras verificar el commit definitivo.
   - Adjuntar en la Release de GitHub los paquetes binarios generados y sus sumas de verificación.
   - No activar publicaciones automáticas en ramas de trabajo o integración.
