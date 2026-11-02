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

### Arquitectura de Instalación Versionada
La instalación en el prefijo del usuario (por defecto `~/.local/`) adopta una topología multiversión con punteros simbólicos atómicos:

```text
~/.local/
├── bin/
│   ├── cadcraft -> ../share/cadcraft/current/bin/cadcraft
│   └── cadcraft-cli -> ../share/cadcraft/current/bin/cadcraft-cli
└── share/
    ├── applications/
    │   └── ai.storyteller.cadcraft.desktop  (Exec="/home/user/.local/bin/cadcraft" %F)
    ├── icons/hicolor/...
    ├── mime/packages/ai.storyteller.cadcraft.xml
    └── cadcraft/
        ├── current -> versions/0.1.0-86312948bbdf9c95d9856adfa39c27ff6640c495
        ├── previous -> versions/0.1.0-7b24ac10...
        ├── install.sh
        ├── uninstall.sh
        ├── version.env
        ├── install-manifest.txt
        ├── .install.lock
        └── versions/
            ├── 0.1.0-86312948bbdf9c95d9856adfa39c27ff6640c495/
            │   ├── bin/ (cadcraft, cadcraft-cli)
            │   ├── share/doc/cadcraft/
            │   └── version.env
            └── 0.1.0-7b24ac10.../
                └── ...
```

### Seguridad y Robustez de los Scripts
- **Identidad inmutable de compilación:** Cada versión se aloja bajo `versions/<version>-<sha>/` donde `<sha>` es el hash de commit Git completo de 40 caracteres hexadecimales. Si existe un directorio con igual identidad pero sumas de comprobación SHA-256 diferentes (incluida cualquier alteración en `cadcraft-cli` o archivos registrados en `build-manifest.txt`), el instalador aborta sin modificar nada, incluso ante `--force`.
- **Inventario por build y titularidad estricta:** Cada compilación mantiene su propio archivo de inventario `versions/<build-id>/build-manifest.txt` con sumas SHA-256 individuales. El instalador no reconstruye la titularidad de archivos mediante `find` sobre el árbol instalado (lo cual podría adoptar archivos ajenos o de usuario). `install-manifest.txt` agrega exclusivamente los archivos acreditados por los manifiestos de las versiones gestionadas y los recursos compartidos del sistema.
- **Tratamiento estricto de metadatos:** `version.env` se procesa como archivo de datos estructurado mediante análisis léxico con validación estricta de expresiones regulares. Se exigen los 4 campos obligatorios generados por `package.sh` (`CADCRAFT_VERSION`, `CADCRAFT_BUILD_SHA` de 40 hex, `CADCRAFT_BUILD_DATE` y `CADCRAFT_ARCH`). Se rechazan claves ausentes, duplicadas, desconocidas o con sintaxis inválida. La compatibilidad con versiones legacy anteriores se maneja mediante un analizador de migración independiente (`parse_legacy_version_env`).
- **Activación atómica por enlaces en el mismo sistema de ficheros:** La conmutación de versión no depende de `ln -sfn`. En su lugar, se crea un enlace temporal en el mismo directorio (`current.tmp.<pid>`) y se reemplaza mediante `mv -Tf` (llamada al sistema atómica `rename` en Linux).
- **Protección contra concurrencia y gestión de lockfile:** Se exige la presencia de la herramienta `flock` tanto en `install.sh` como en `uninstall.sh`. Si falta en `$PATH`, la operación aborta antes de modificar el destino. El bloqueo exclusivo sobre `.install.lock` se mantiene durante toda la operación y durante cualquier recuperación tras error. El archivo de bloqueo no se elimina al finalizar ni durante la ejecución para evitar que procesos concurrentes abran inodos diferentes y adquieran bloqueos paralelos.
- **Inspección obligatoria de procesos:** Se comprueba si `cadcraft` o `cadcraft-cli` están en ejecución inspeccionando `/proc/<pid>/exe`. No se permite bypass mediante `--force`.
- **Smoke tests obligatorios previos a la activación:** Todo binario nuevo pasa una comprobación `--version` con límite de tiempo (timeout) en su directorio de staging antes de tocar la versión activa o enlaces del sistema.
- **Contención física y verificación de límites de directorio:** Se verifica la contención estricta mediante límites reales de directorio (utilizando delimitadores `/` y resolución canónica, rechazando prefijos textuales ambiguos). Se comprueba que los directorios padres gestionados (`bin/`, `share/`, `share/cadcraft/`, `versions/`) no sean enlaces simbólicos que apunten fuera del prefijo de instalación.
- **Recuperación transaccional ante errores ordinarios:** Antes de realizar mutaciones, se registra un snapshot del estado previo (`current`, `previous`, binarios de entrada en `bin/`, metadatos, manifiestos y recursos de escritorio/iconos compartidos). Ante cualquier fallo en activación, integración, migración o rollback, el manejador `recover_transaction` restaura el estado previo y limpia artefactos parciales manteniendo el bloqueo activo. Si la recuperación fallase, se notifica y se preserva el directorio de respaldo `.recovery.<pid>`.
- **Migración segura de instalaciones anteriores:** La migración desde esquemas planos legacy se ejecuta únicamente después de superar la prevalidación y obtener confirmación del usuario. Solo se migran los archivos acreditados en el manifiesto anterior. Si la migración o activación fallan, se restaura la instalación plana previa.
- **Preservación de archivos ajenos:** Al desinstalar o actualizar, se eliminan únicamente los archivos registrados en el manifiesto. Los directorios de versiones y carpetas compartidas solo se podan con `rmdir` si quedan completamente vacíos; cualquier archivo creado por el usuario o ajeno a CADCraft se preserva intacto.
- **Detección estricta de colisiones y alcance de `--force`:** Un archivo preexistente en el destino se considera gestionado única y exclusivamente si está acreditado por un manifiesto `install-manifest.txt` válido de una instalación previa (rutas relativas estrictas, sin `..` y con resolución física dentro del prefijo). Se han eliminado deliberadamente heurísticas permisivas por prefijo (`share/cadcraft/`), nombre (`ai.storyteller.cadcraft`) o texto de enlaces. El modificador `--force` permite únicamente sobrescribir archivos no gestionados que colisionen dentro del prefijo. **Bajo ninguna circunstancia `--force` permite:**
  1. Saltarse la contención física ni sobrescribir enlaces que resuelvan fuera del prefijo.
  2. Omitir la comprobación de procesos activos (`cadcraft` o `cadcraft-cli`).
  3. Sobrescribir un directorio de versión existente que posea la misma identidad (`version-sha`) pero diferente contenido.
- **Poda automática desactivada:** La eliminación automática de versiones anteriores está deliberadamente desactivada para evitar pérdidas no controladas.

### Limitaciones de Recuperación ante Eventos No Enmascarables
Es fundamental distinguir la recuperación ante errores ordinarios de las limitaciones físicas y del sistema operativo:
- **Errores ordinarios cubiertos:** Fallos de permisos, comandos que retornan código de salida distinto de cero, interrupciones manuales por el usuario (`SIGINT`, Ctrl+C), o señales de terminación ordenada (`SIGTERM`) son interceptados por el trap `EXIT` de bash, ejecutando `recover_transaction` y restaurando el estado previo de manera íntegra.
- **Eventos no enmascarables:** Una señal `SIGKILL` (enviada por un administrador o por el Linux Out-Of-Memory Killer) o un corte súbito de alimentación eléctrica / cuelgue de kernel interrumpen inmediatamente el proceso sin permitir la ejecución de manejadores de señal o rutinas de salida de la shell. En tales circunstancias:
  - Los enlaces simbólicos modificados mediante `mv -Tf` son atómicos a nivel de sistema de ficheros: apuntarán o bien al destino antiguo o bien al nuevo, nunca a un estado corrupto a medias.
  - El directorio temporal de staging (`versions/<build>.staging.<pid>`) o el directorio de recuperación (`.recovery.<pid>`) pueden permanecer en disco como basura inerte.
  - **Limitación pendiente de saneamiento post-corte:** Si un corte abrupto o `SIGKILL` interrumpe el proceso exactamente entre la conmutación de enlaces y la escritura final del manifiesto/metadatos, la reconciliación automática de dicho estado intermedio no está probada exhaustivamente. El saneamiento automático integral ante fallos de energía o `SIGKILL` permanece como una limitación pendiente para etapas posteriores.

---

## 3. Procedimiento de Instalación, Actualización y Rollback

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
1. Valida el paquete y prepara la versión en `~/.local/share/cadcraft/versions/<version>-<sha>/`.
2. Actualiza atómicamente el enlace `current`.
3. Crea los enlaces simbólicos estables en `~/.local/bin/cadcraft`.
4. Instala la entrada `.desktop` en `~/.local/share/applications/` con ruta absoluta real.
5. Copia la colección de iconos y definiciones MIME.
6. Registra el manifiesto en `~/.local/share/cadcraft/install-manifest.txt`.
7. Ejecuta `update-desktop-database`, `gtk-update-icon-cache` y `update-mime-database`.

### B. Actualización de Versión
Para actualizar una instalación existente conservando dibujos, plantillas y configuración:
```bash
tar -xzf cadcraft-<nueva-version>-linux-x86_64.tar.gz
cd cadcraft-<nueva-version>-linux-x86_64
./install.sh --update
```
- Valida la nueva compilación antes de activarla.
- Apunta `previous` a la versión activa anterior y `current` a la nueva versión.
- Si CADCraft está abierto, exige cerrarlo antes de proceder.
- Mantiene intactos los archivos de configuración (`~/.config/cadcraft`), estado de sesión (`~/.local/share/ai.storyteller.cadcraft`), y dibujos.

### C. Reversión Explícita (Rollback)
Si una nueva versión presenta incompatibilidades o problemas, es posible regresar a la versión previa inmediatamente.
En la instalación por defecto (`~/.local/`):
```bash
~/.local/share/cadcraft/install.sh --rollback
```
Si se utilizó un **prefijo de instalación personalizado**, la implementación actual requiere pasar explícitamente dicho prefijo con `--prefix`:
```bash
/ruta/personalizada/share/cadcraft/install.sh --prefix /ruta/personalizada --rollback
```
- Conmuta de forma atómica el enlace `current` a la versión referenciada por `previous`.
- Apunta `previous` a la versión anterior para permitir conmutación bidireccional si se repite el comando.
- Actualiza los enlaces ejecutables en `bin/`.
- No requiere descargar de nuevo la versión anterior ni disponer del archivo tar original.

> [!WARNING]
> **Rollback de binarios vs. migración de configuración:** La reversión devuelve los ejecutables al estado binario de la versión anterior. Si la versión más reciente realizó modificaciones o migraciones en el formato de `workspace.json`, guardó dibujos con esquemas nuevos o alteró la configuración en `~/.config/cadcraft/`, el rollback de binarios **no revierte automáticamente esas transformaciones de datos**. Se recomienda realizar copias de seguridad de dibujos críticos antes de probar versiones mayores.

### D. Comprobación del Menú de Aplicaciones
La mayoría de entornos de escritorio modernos (GNOME, KDE Plasma, XFCE) actualizan el menú de aplicaciones en segundo plano de forma automática. Si el icono o lanzador de CADCraft no aparece inmediatamente:
- **GNOME en X11:** Pulsar `Alt+F2`, escribir `r` y pulsar `Enter`.
- **GNOME en Wayland:** Cerrar y volver a iniciar la sesión de usuario, o ejecutar:
  ```bash
  update-desktop-database ~/.local/share/applications
  ```
- **KDE Plasma:** Ejecutar `kbuildsycoca5` (o `kbuildsycoca6` en Plasma 6) o reiniciar el shell de Plasma.

### E. Desinstalación Limpia
```bash
~/.local/share/cadcraft/uninstall.sh
# O ejecutando ./uninstall.sh desde la carpeta del paquete
```
- Lee `install-manifest.txt` y borra únicamente los archivos instalados por CADCraft.
- Elimina los enlaces simbólicos en `~/.local/bin/` y las versiones en `versions/`.
- No utiliza borrado recursivo indiscriminado (`rm -rf`) sobre carpetas compartidas: utiliza `rmdir` para que cualquier fichero ajeno o de usuario que se haya depositado en la carpeta quede intacto.
- Actualiza las bases de datos de escritorio y MIME para retirar el acceso del menú.
- Conserva intactos todos los documentos personales y configuraciones de usuario.

---

## 4. Conservación de Preferencias y Datos

CADCraft separa estrictamente los binarios de la configuración según los estándares XDG:
- **Configuración, logs y autosave:** `$XDG_CONFIG_HOME/cadcraft/` (por defecto `~/.config/cadcraft/`). Contiene `workspace.json`, logs de ejecución y archivos de autorrecuperación (`autosave/`).
- **Almacén de estado y vistas:** `$XDG_DATA_HOME/ai.storyteller.cadcraft/` (por defecto `~/.local/share/ai.storyteller.cadcraft/app.ron`). Guarda estado de ventanas, paneles y geometría de egui.
- **Documentos de usuario:** Guardados exclusivamente en las carpetas personales elegidas por el usuario (`.dwg`, `.dxf`, `.dwt`), sin residir nunca en el árbol del instalador.
- **Árbol de instalación:** `$PREFIX/share/cadcraft/` contiene exclusivamente ejecutables empaquetados, manifiestos, scripts y enlaces de control. No almacena datos de usuario.

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
