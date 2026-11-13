# Comparación Focalizada de Flujos Clave: CADcraft vs. Autodesk AutoCAD

Este documento presenta una auditoría técnica y funcional verificable de seis flujos de trabajo críticos comparando la implementación real de **CADcraft** frente a **Autodesk AutoCAD** (versiones de referencia AutoCAD 2024 / 2025).

Cada flujo se clasifica según los criterios establecidos:
- **Existe y funciona**
- **Existe pero necesita mejora**
- **Falta**
- **Fuera de alcance por ahora**

---

## 1. Crear desde plantilla

### Clasificación
- **Existe y funciona** (separación estricta de acciones, no creación de documento al configurar la predeterminada, guardado forzado a SAVEAS).
- **Existe pero necesita mejora** (no lee metadatos descriptivos ni bloques incrustados de archivos binarios propietarios `.dwt` complejos).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/engine/src/cmd/file/template.rs`: comandos `new`, `run_new`, `run_new_template`, `qnew_template`, `set_qnew_template`.
  - `crates/ui-egui/src/menus.rs`: comandos `ui.defaulttemplate.choose` (valida y fija la preferencia sin abrir documentos) y `ui.defaulttemplate.clear` (restablece a dibujo en blanco).
  - `crates/ui-egui/src/chrome.rs`: interfaz de la barra lateral de Inicio (Start Page) con botones "Choose...", "Clear" y visualización de la plantilla activa.
  - Tests automatizados: `crates/engine/tests/templates.rs` (`qnewtemplate_is_the_default_for_new_and_qnew`, `default_qnewtemplate_unreadable_falls_back_safely_to_blank_drawing`) y `crates/ui-egui/tests/it/integrated_ui_validation.rs` (`test_template_ui_journey_with_simulated_pickers`).
- **Comportamiento implementado:**
  - **Elegir plantilla predeterminada:** Comprueba la validez de la ruta, actualiza la variable de sistema `QNEWTEMPLATE` y la persiste en `prefs.json`. No abre ni crea ningún documento nuevo en el espacio de trabajo.
  - **Nuevo desde otra plantilla:** Abre un documento independiente basado en el contenido del archivo seleccionado sin modificar la plantilla predeterminada del usuario.
  - **Nuevo (`Ctrl+N` / `Cmd+N` / `QNEW`):** Utiliza automáticamente el contenido de la plantilla predeterminada o genera un dibujo en blanco si no hay ninguna configurada.
  - **Quitar plantilla predeterminada:** Limpia la preferencia y persiste el estado vacío.
  - **Aislamiento de guardado:** Todo documento derivado de plantilla nace sin ruta física asociada (`doc.path.is_none()`); el primer guardado (`QSAVE` o `Cmd+S`) invoca forzosamente el diálogo `SAVEAS`.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "To Specify a Default Drawing Template File":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-42BCFE74-4C3D-4A2B-8DF0-38ACCD8B5F9C](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-42BCFE74-4C3D-4A2B-8DF0-38ACCD8B5F9C)
  - *Autodesk AutoCAD 2024 Help — "QNEW (Command)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-E5A8E1D9-D582-4C69-90CE-E03541A066C4](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-E5A8E1D9-D582-4C69-90CE-E03541A066C4)
- **Comportamiento documentado:** AutoCAD configura la plantilla por defecto a través de `Options ▸ Files ▸ Template Settings ▸ Default Template File Name for QNEW`. Al ejecutar `QNEW`, carga la plantilla configurada e inicializa `DrawingN.dwg` sin ruta de destino, requiriendo `SAVEAS` en el primer guardado.

### Inferencias y diferencias de diseño
- *Inferencia / Diferencia:* En AutoCAD, las plantillas son archivos `.dwt` con marcas de cabecera binarias específicas. CADcraft admite cualquier archivo `.dwg` o `.dxf` válido como plantilla sin exigir una extensión exclusiva `.dwt`. Si la plantilla no se puede leer, CADcraft inicia con un dibujo en blanco seguro reportando un aviso informativo.

---

## 2. Buscar y abrir recientes y generación de miniaturas

### Clasificación
- **Existe pero necesita mejora** (las miniaturas 2D capturan la vista guardada, pero existe el límite documentado de rasterización 2D frente a cámaras 3D; la fecha visualizada corresponde a la modificación del archivo en disco).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/ui-egui/src/thumbnails.rs`: funciones `update_thumbnail` (con escala y centro derivados de la vista activa `st.view()`), `invalidate_thumbnail_texture`, `prune_disk_cache`, `load_thumbnail_texture`.
  - `crates/ui-egui/src/chrome.rs`: presentación de cuadrícula de tarjetas (`Card Grid`) y lista, filtrado interactivo, etiqueta explícita `Modified: {fecha}` y preservación de favoritos (`pinned_files`).
  - Tests automatizados: `crates/ui-egui/tests/recent_files.rs` (invalidación de textura en memoria, cobertura de guardado por menú, atajo y comando, preservación de favoritos ante desbordamiento de recientes, tolerancia a caché corrupta o ausente).
- **Comportamiento implementado:**
  - La tarjeta en la pantalla de Inicio se actualiza en la misma sesión tras guardar, invalidando la textura en memoria previa (`app.invalidate_thumbnail`).
  - La lista de recientes y favoritos se fusiona para que los archivos fijados se mantengan visibles incluso si caen fuera de los N recientes no fijados.
  - La caché en disco está limitada (`prune_disk_cache(100)`).
  - La fecha se etiqueta explícitamente como modificación del archivo (`Modified: ...`).

### Límite de renderizado documentado (no construir nuevo motor)
- **Límite 2D vs. Cámara 3D:** El motor de renderizado de miniaturas (`cadcraft_render::raster::render_png`) opera sobre listas de visualización 2D (`DisplayList`), ajustando la escala y el centro a la vista activa del documento (`st.view().center` y `st.view().height`). No se dispone de un rasterizador por software 3D autónomo ni de captura del framebuffer de la GPU fuera de pantalla; por tanto, para vistas 3D axonométricas o en perspectiva, la miniatura refleja la proyección plana y no una captura 3D sombreada fotorrealista.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "About the Start Tab":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-A641A758-BD26-4447-B27D-5EB9A3CF5709](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-A641A758-BD26-4447-B27D-5EB9A3CF5709)
  - *Autodesk AutoCAD 2024 Help — "To Work with Recent Documents on the Start Tab":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-9F4F6A24-F4E5-46D9-B58A-AC3EF0529D86](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-9F4F6A24-F4E5-46D9-B58A-AC3EF0529D86)
- **Comportamiento documentado:** AutoCAD almacena la miniatura en una sección de mapa de bits (BMP/PNG) incrustada dentro del propio archivo DWG al guardar, que la pantalla de inicio lee directamente.

### Inferencias y diferencias de diseño
- *Inferencia / Diferencia:* CADcraft almacena las miniaturas de forma desacoplada en el directorio de caché de usuario (`$XDG_CACHE_HOME/cadcraft/thumbnails/`) indexadas por hash FNV-1a de la ruta absoluta para evitar mutar los bytes de dibujos DWG de solo lectura o procedentes de clientes externos.

---

## 3. Introducir coordenadas y entrada dinámica (XYZ)

### Clasificación
- **Existe y funciona** (introducción de coordenadas cartesianas 2D y 3D, navegación con Tab/Shift+Tab, etiquetas de plano X/Y y X/Z, prefijos `@`, `#`, `*`, `@*`, preservación estricta de Z en coordenadas tecleadas y contención en bordes).
- **Existe pero necesita mejora** (conmutación automática a campos polares distancia/ángulo durante comandos interactivos dinámicos).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/ui-egui/src/dyninput.rs`: función `parse_cartesian`, soporte de prefijos `@`, `#`, `*`, `*@`, conteo de componentes y actualización de campos individuales.
  - `crates/ui-egui/src/cmdline.rs`: navegación interactiva `tab` y `tab_backward` (Shift+Tab), edición de componentes individuales con Backspace y confirmación en canvas.
  - `crates/ui-egui/src/canvas.rs`: `dynamic_input_boxes` con resaltado del campo activo, `tooltip_frame` con contención geométrica estricta en los cuatro bordes (margen de 4px) y adaptabilidad a ventanas reducidas; previsualización geométrica 3D consistente con Z tecleado.
  - Tests automatizados: `integrated_ui_validation::test_dynamic_input_harness_events_and_containment`, `test_coordinates_3d_explicit_and_relative_preserve_z`.
- **Comportamiento implementado:**
  - Al teclear una segunda coma (`0,0,0` o `@5,5,10`), aparece de forma inmediata el tercer campo correspondiente a Z.
  - En vista Frontal (plano XZ), dos componentes se etiquetan como X y Z; con tres componentes se etiquetan X, Y, Z.
  - Las teclas `Tab` y `Shift+Tab` permiten desplazarse cíclicamente entre los campos sin destruirlos. `Escape` cancela la entrada.
  - Contención: el widget no sobrepasa ninguno de los cuatro bordes de la ventana gráfica.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "About Entering Coordinates with Dynamic Input":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-6C8DC3E5-A1E4-42B2-9383-74EB45EB93CA](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-6C8DC3E5-A1E4-42B2-9383-74EB45EB93CA)
  - *Autodesk AutoCAD 2024 Help — "About Entering 3D Cartesian Coordinates":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-45733C14-3A58-45BC-B0D2-E40DF48B2521](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-45733C14-3A58-45BC-B0D2-E40DF48B2521)
  - *Autodesk AutoCAD 2024 Help — "DYNMODE (System Variable)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-E885B464-9669-42EB-85F0-5517E9110B6B](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-E885B464-9669-42EB-85F0-5517E9110B6B)
- **Comportamiento documentado:** AutoCAD presenta campos de entrada dinámica donde Tab conmuta entre coordenadas o entre distancia y ángulo. Al escribir comas, se pasa de X a Y y a Z.

### Inferencias y diferencias de diseño
- *Inferencia / Regla geométrica 2D/3D:* En clics libres con ratón en vista oblicua 3D sin referencia OSNAP, CADcraft proyecta sobre el plano de construcción Z=0. Sin embargo, **toda coordenada introducida mediante teclado con Z explícito (absoluto o relativo) conserva su cota Z exacta en el modelo 3D**, sin forzarse a cero.

---

## 4. Seleccionar y modificar geometría

### Clasificación
- **Existe pero necesita mejora** (selección por ventana, cruce y comandos de transformación implementados; falta selección por lazo y menú contextual de pinzamientos).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/ui-egui/src/canvas.rs`: lógica de ventana rectangular (`pick_rect`), ventana inclusiva (azul) y ventana de cruce (verde), pinzamientos interactivos (`draw_grips`).
  - `crates/engine/src/cmd/modify/`: comandos `move.rs`, `copy.rs`, `rotate.rs`, `scale.rs`, `mirror.rs`, `erase.rs`, `trim.rs`, `extend.rs`, `fillet.rs`.
- **Comportamiento implementado:**
  - Selección por clic simple sobre entidad con tolerancia configurable.
  - Selección de ventana inclusiva (arrastre izquierda a derecha) y cruce (arrastre derecha a izquierda).
  - Pinzamientos interactivos para estirar o desplazar puntos clave.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "About Selecting Objects":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-898EB0A8-C9F2-494F-B23B-CFBE5D6E3F10](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-898EB0A8-C9F2-494F-B23B-CFBE5D6E3F10)
  - *Autodesk AutoCAD 2024 Help — "About Editing with Grips":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-B5706597-9092-4B44-9B02-C755716E823E](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-B5706597-9092-4B44-9B02-C755716E823E)
- **Comportamiento documentado:** AutoCAD incluye selección por lazo continuo (manteniendo presionado el botón izquierdo), selección por polígono (`WP`, `CP`), cerca (`FENCE`) y menús flotantes al posar el cursor sobre pinzamientos en arcos, splines y polilíneas.

---

## 5. Guardar, cerrar y reabrir

### Clasificación
- **Existe y funciona** (`QSAVE`, `SAVEAS`, control de cambios pendientes al cerrar, regeneración e invalidación de miniatura tras guardado exitoso).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/engine/src/cmd/file/save.rs`: comandos `qsave`, `saveas`.
  - `crates/ui-egui/src/menus.rs`: `ui.saveas`.
  - `crates/ui-egui/src/closing.rs`: diálogo modal interactivo ante cambios pendientes.
  - `crates/ui-egui/tests/recent_files.rs`: `test_save_coverage_menu_shortcut_cmdline` e invalidación tras fallo o éxito de guardado.
- **Comportamiento implementado:**
  - `QSAVE` (`Cmd+S` / `Ctrl+S`): guarda sobre el archivo si tiene ruta; si es un documento nuevo o derivado de plantilla, abre `SAVEAS`.
  - Si el guardado falla, la miniatura previa no se altera ni se reemplaza erróneamente.
  - Guardado con éxito invalida la textura previa y actualiza la tarjeta en la sesión actual.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "QSAVE (Command)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-D53BA5E3-F25B-4C2E-862F-8ACAC38699EB](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-D53BA5E3-F25B-4C2E-862F-8ACAC38699EB)
  - *Autodesk AutoCAD 2024 Help — "SAVEAS (Command)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-4017EE2D-FBD3-40D3-A9CE-5C0D30FF5F9C](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-4017EE2D-FBD3-40D3-A9CE-5C0D30FF5F9C)

---

## 6. Preparar PDF o impresión

### Clasificación
- **Existe pero necesita mejora** (exportación directa a PDF y trazado desde espacio modelo disponibles; espacio papel / layouts avanzados en desarrollo).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/engine/src/cmd/plot.rs`: comandos `plot`, `exportpdf`.
  - `crates/render/src/plot/`: pipeline de vectorización de trazado a PDF.
  - `crates/ui-egui/src/menus.rs`: interceptor `exportpdf`.
- **Comportamiento implementado:**
  - `EXPORTPDF`: genera PDF vectorial del encuadre actual o extensiones del modelo.
  - `PLOT`: diálogo con selección de tamaño de papel, orientación y asignación básica de plumillas CTB.

### Referencia documentada oficial en AutoCAD
- **URLs consultadas:**
  - *Autodesk AutoCAD 2024 Help — "PLOT (Command)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-9EB3D7EE-FA91-4DDE-99F7-41BA52960E21](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-9EB3D7EE-FA91-4DDE-99F7-41BA52960E21)
  - *Autodesk AutoCAD 2024 Help — "EXPORTPDF (Command)":* [https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-002E709F-1A5F-4DF3-9EBD-A2EE24C03348](https://help.autodesk.com/view/ACD/2024/ENU/?guid=GUID-002E709F-1A5F-4DF3-9EBD-A2EE24C03348)

---

## 7. Modelado 3D básico y preparación para impresión (EXTRUDE, STLOUT, COPYBASE)

### Clasificación
- **Existe pero necesita mejora** (recorrido 3D básico y exportación STL de sólidos simples plenamente validados en slicer; la discretización de intersecciones con curvas a tolerancia fina de 0.05 mm permanece como limitación abierta).

### Evidencia en CADcraft
- **Archivos y funciones:**
  - `crates/engine/src/cmd/solids.rs`: comando `EXTRUDE` con activación automática del modo de visualización Conceptual.
  - `crates/engine/src/cmd/exchange.rs`: comando `STLOUT` con validación estricta de manifoldness de malla binaria STL.
  - `crates/ui-egui/src/exchange.rs`: integración gráfica de diálogo nativo de guardar archivo con adición automática de extensión `.stl`.
  - `crates/engine/src/cmd/edit.rs`, `crates/engine/src/cmd/clipboard.rs`: comando `COPYBASE` (`Ctrl+Shift+C`) con punto base 3D y conservación de recetas B-Rep de `Solid3d`.
  - Tests automatizados: `crates/engine/tests/it/extrude_stl_mm.rs`, `crates/engine/src/cmd/clipboard_tests.rs`, `crates/ui-egui/src/clipboard.rs`.

### Estado Validado
- **Milímetros para dibujos nuevos y exportación física probada:** Sistema predeterminado en milímetros para plantillas y cotas, garantizando escala 1:1 en slicers.
- **Placa simple desde interfaz hasta OrcaSlicer:** Flujo `RECTANG` → `EXTRUDE` completado e importado con éxito en OrcaSlicer con dimensiones nominales exactas (40 × 30 × 5 mm).
- **Sombreado automático tras EXTRUDE:** Activación automática del estilo visual Conceptual tras completar la operación de extrusión.
- **STLOUT con diálogo y extensión automática:** Apertura de selector de archivos nativo con anexado automático de extensión `.stl` y cabecera neutra (`axialCAD binary STL`).
- **COPYBASE y copia de sólidos entre dibujos:** Copia relativa con punto base 3D (`Ctrl+Shift+C`), preservación de la receta B-Rep y volumen de `Solid3d`, e inserción en el mismo dibujo y entre dibujos distintos.

### Limitaciones Abiertas
- **El caso de placa perforada falla la validación a 0.05 mm:** El mallador genera aristas abiertas (discrepancias entre subdivisiones de caras adyacentes) en la costura cilíndrica a tolerancia fina, impidiendo superar la validación estricta de manifoldness de STL.
- **Casos ensayados a 0.15 / 0.25 mm:** Aunque superan la validación geométrica y pueden exportarse, el agujero se muestra visiblemente poligonal en OrcaSlicer en comparación con modelos de referencia, por lo que no resuelven el objetivo de calidad fina.
- **Postprocesado experimental:** El saneamiento geométrico posterior sobre `TriangleMesh` no produjo una solución válida al carecer de acceso a las curvas analíticas B-Rep.
- **Corrección del mallador:** No existe todavía una corrección aprobada del mallador en la fase de discretización previa.

---

## Resumen de Estado y Clasificación

| Flujo | Clasificación Actualizada | Límite o Trabajo Pendiente |
|---|---|---|
| **1. Plantilla personal** | **Existe y funciona** | Lectura de cadenas descriptivas incrustadas en cabeceras propietarias DWT. |
| **2. Inicio y miniaturas** | **Existe pero necesita mejora** | Renderizador limitado al pipeline 2D (no software 3D raster); fecha visualizada es de modificación en disco. |
| **3. Entrada XYZ y precisión** | **Existe y funciona** | Conmutación interactiva a entradas polares en tiempo de arrastre de comando. |
| **4. Selección y modificación** | **Existe pero necesita mejora** | Selección por lazo continuo (lasso) y menús contextuales hover en pinzamientos. |
| **5. Guardar / cerrar / reabrir** | **Existe y funciona** | Flujo completo de comprobación de guardado, diálogos de cierre e invalidación de caché. |
| **6. Trazado / PDF** | **Existe pero necesita mejora** | Espacio papel con múltiples viewports y publicación por lotes (`PUBLISH`). |
| **7. Modelado 3D y STL** | **Existe pero necesita mejora** | Recorrido simple validado en slicer; mallado conforme a tolerancia fina (0.05 mm) en sólidos perforados pendiente de corrección en kernel. |
