# Distribución y Empaquetado Linux de CADcraft

Este documento describe la arquitectura de empaquetado, la estructura de artefactos generados, las dependencias del sistema, el ciclo de vida de configuración y las directrices para la validación y lanzamiento de paquetes.

---

## 1. Formatos de Entrega y Alcance de Compatibilidad

En el estado actual de la plataforma, el formato principal verificado es el **Tarball FHS Portable (`.tar.gz`)**:

| Formato | Estado de Validación | Dependencias del Sistema | Comportamiento y Despliegue |
|---|---|---|---|
| **Tarball FHS Portable** (`.tar.gz`) | **Construido y verificado localmente** en Linux x86_64 (CachyOS) | Bibliotecas del sistema del anfitrión de compilación: glibc compatible, stack gráfico (Wayland / X11, Mesa/Vulkan, libxkbcommon) | Árbol portable estándar FHS (`bin/`, `share/applications/`, `share/icons/`, `share/mime/`). Puede desplegarse en `~/.local/` o en directorios aislados como `/opt/cadcraft/`. |
| **AppImage** (`.AppImage`) | **No validado como entrega comprobada** (script disponible, pero sin ejecución ni validación multi-distribución en esta entrega) | Runtime AppImage, FUSE / libfuse2 o `APPIMAGE_EXTRACT_AND_RUN=1` | Formato opcional definido en `package.sh`; requiere validación formal en entornos limpios antes de considerarse entregable oficial. |

### Niveles de Comprobación Técnicos
- **Construcción:** Se realiza mediante `cargo build --release` empaquetando `cadcraft` y `cadcraft-cli`.
- **Arranque Gráfico y de Control:** Verificado en el entorno de desarrollo local (ejecución gráfica con winit/wgpu/egui, paso de argumentos, carga de preferencias e IPC de control).
- **Validación entre Distribuciones:** No se asumen garantías automáticas de compatibilidad universal (ej. Fedora u otras distribuciones con versiones dispares de glibc o dependencias gráficas) sin ejecución previa en matrices de CI o contenedores de prueba específicos para cada distribución.

---

## 2. Estructura del Paquete Tarball e Integración de Escritorio

El archivo tarball generado (`cadcraft-<version>-linux-<arch>.tar.gz`) contiene una carpeta raíz con la siguiente jerarquía FHS:

```text
cadcraft-<version>-linux-<arch>/
├── bin/
│   ├── cadcraft
│   └── cadcraft-cli
└── share/
    ├── applications/
    │   └── ai.storyteller.cadcraft.desktop
    ├── doc/
    │   └── cadcraft/
    ├── icons/
    │   └── hicolor/
    └── mime/
        └── packages/
            └── ai.storyteller.cadcraft.xml
```

### Integración en el Escritorio
- **Identificador de aplicación:** `ai.storyteller.cadcraft`
- **Archivo `.desktop`:**
  - `Exec=cadcraft %F`
  - `TryExec=cadcraft`
  - `Icon=ai.storyteller.cadcraft`
- **Requisito de resolución de ruta (`Exec`):**
  Al extraer mediante `--strip-components=1` en `~/.local/`, los ejecutables se sitúan en `~/.local/bin/`. Para que el gestor de ventanas y el archivo `.desktop` localicen el ejecutable mediante `Exec=cadcraft`, **el directorio `~/.local/bin` debe encontrarse en la variable de entorno `$PATH` del usuario**. Si la distribución no incluye `~/.local/bin` en `$PATH` por defecto, el usuario debe incorporarlo a su perfil de sesión o crear un enlace simbólico en un directorio de sistema como `/usr/local/bin/`.

---

## 3. Conservación de Preferencias y Datos de Usuario

CADcraft separa estrictamente el código binario de la configuración del usuario según la especificación *XDG Base Directory*:

- **Preferencias de usuario:** `$XDG_CONFIG_HOME/cadcraft/prefs.json` (por defecto `~/.config/cadcraft/prefs.json`).
  - Almacena idioma, plantilla por defecto (`QNEWTEMPLATE`), formato y versión de guardado, historial de recientes y archivos fijados (`pinnedFiles`).
- **Caché de miniaturas:** `$XDG_CACHE_HOME/cadcraft/thumbnails/` (por defecto `~/.cache/cadcraft/thumbnails/`).
  - Miniaturas rasterizadas asociadas al hash y modificación de los archivos.

---

## 4. Instrucciones de Instalación y Desinstalación

### A. Despliegue de Usuario (Tarball)
```bash
# 1. Extraer los componentes en ~/.local
tar -xzf cadcraft-*-linux-x86_64.tar.gz -C ~/.local/ --strip-components=1

# 2. Asegurar que ~/.local/bin está en el PATH
export PATH="$HOME/.local/bin:$PATH"

# 3. Actualizar bases de datos de escritorio y MIME del entorno de usuario
update-desktop-database ~/.local/share/applications/
update-mime-database ~/.local/share/mime/
gtk-update-icon-cache -f ~/.local/share/icons/hicolor/
```

### B. Desinstalación Normal
La desinstalación regular retira únicamente los archivos binarios e integraciones del sistema, preservando siempre la configuración y documentos del usuario:
```bash
rm -f ~/.local/bin/cadcraft ~/.local/bin/cadcraft-cli
rm -f ~/.local/share/applications/ai.storyteller.cadcraft.desktop
rm -f ~/.local/share/mime/packages/ai.storyteller.cadcraft.xml
rm -rf ~/.local/share/doc/cadcraft
```
> **Nota de integridad:** No se debe sugerir el borrado de `~/.config/cadcraft` como parte del proceso estándar de desinstalación, a fin de evitar la pérdida accidental de preferencias, plantillas personalizadas o listas de archivos de trabajo.

---

## 5. Procedimiento de Empaquetado y Publicación

1. **Compilación y generación del paquete:**
   El paquete definitivo se genera únicamente tras validar todas las pruebas de regresión y crear el commit definitivo en la rama correspondiente.
   ```bash
   ./packaging/linux/package.sh --formats "tar"
   ```
2. **Identificación de Versión y Estado del Árbol:**
   El script de empaquetado incrusta el hash de commit y detecta si el árbol de trabajo contiene modificaciones locales (`-dirty`), reportándolo mediante `--version`:
   ```bash
   cadcraft --version
   # Salida: cadcraft <version> (<commit>[-dirty])
   ```
3. **Reglas de Publicación:**
   - No publicar ni sobrescribir etiquetas de versión (tags) ya existentes en el repositorio.
   - Todo paquete distribuido debe corresponder a un commit auditado y validado en su suite completa de pruebas.
