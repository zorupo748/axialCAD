# CADCraft 3D — Especificación del Contrato Command-First para JARVIS y Persistencia en Documento

Esta especificación establece el estándar arquitectónico que regirá la integración de operaciones 3D en las fases subsiguientes de CADCraft.

---

## 1. Principio Fundamental: Single Motor, No Parallel Engines

No se crearán APIs o controladores paralelos exclusivos para IA. Toda operación de modelado 3D (creación de primitivas, extrusión, revolución, redondeos, uniones, sustracciones e intersecciones) se registrará como un comando de primer orden en el catálogo universal de `cadcraft-engine` (`CommandSpec`).

Esto garantiza que:
1. **La interfaz de usuario (egui)**: Invoca los comandos a través de `Session::execute` o su máquina de estados interactiva.
2. **La interfaz por línea de comandos (CLI `cadcraft-cli run --cmd`)**: Invoca los mismos comandos mediante JSON estructurado.
3. **El servidor MCP (`cadcraft-mcp`)**: Expone las mismas capacidades a JARVIS mediante la herramienta autorizada `execute`.

---

## 2. Contrato de Entrada y Parámetros Estructurados

Cada comando 3D dispondrá de una especificación JSON declarativa estricta (`spec.params`). No se admitirá que un cliente deba simular eventos de teclado o interpretar texto no estructurado.

Ejemplos de esquemas de parámetros:
- `solid.box`:
  ```json
  {
    "origin": [0.0, 0.0, 0.0],
    "size": [100.0, 100.0, 50.0]
  }
  ```
- `solid.cylinder`:
  ```json
  {
    "center": [50.0, 50.0, 0.0],
    "radius": 10.0,
    "height": 50.0,
    "axis": [0.0, 0.0, 1.0]
  }
  ```
- `solid.subtract`:
  ```json
  {
    "target": "10A",
    "tools": ["10B"],
    "oversize_through": true
  }
  ```

---

## 3. Contrato de Respuesta Determinista

El valor devuelto por `Session::execute(id, params)` para operaciones 3D seguirá un formato estructurado y tipado:

```json
{
  "status": "ok",
  "command": "solid.subtract",
  "affected": {
    "created": ["10C"],
    "modified": [],
    "deleted": ["10A", "10B"]
  },
  "metrics": {
    "volume": 484292.0367,
    "bbox": {
      "min": [0.0, 0.0, 0.0],
      "max": [100.0, 100.0, 50.0]
    }
  },
  "revision": 24
}
```

En caso de error geométrico o de convergencia:
```json
{
  "status": "error",
  "command": "solid.subtract",
  "error_code": "NON_TRANSVERSAL_INTERSECTION",
  "message": "Boolean difference requires transversal intersection; coincident planar faces detected. Use through-cut tool oversizing.",
  "revision": 23
}
```

---

## 4. Contrato de Persistencia y Representación en Documento

Para desacoplar el modelo del documento respecto a versiones internas del kernel de modelado, se establecen tres capas claramente diferenciadas:

```
+-----------------------------------------------------------------------------------------+
|                                ARQUITECTURA DE PERSISTENCIA                             |
|                                                                                         |
|  1. Receta Paramétrica (Canónica)       2. B-Rep Evaluado (Memoria)   3. Malla Derivada |
|  +------------------------------+       +------------------------+    +---------------+ |
|  | FeatureGraph / Recipe:       | ----> | BrepSolid (L2 brep):   | -> | TriangleMesh: | |
|  | - Box(100, 100, 50)          |       | - Faces NURBS          |    | - Triángulos  | |
|  | - Hole(r=10, depth=through)  |       | - IntersectionCurves   |    | - Normales    | |
|  +------------------------------+       +------------------------+    +---------------+ |
|                 |                                                              |        |
|                 v                                                              v        |
|     cadcraft-doc (DXF/DWG/JSON)                                            Render / STL |
+-----------------------------------------------------------------------------------------+
```

### 4.1. Definición Neutral en `cadcraft-doc`
En el siguiente hito, `cadcraft-doc` añadirá la variante `EntityKind::Solid3d(Solid3dRecord)`. La definición neutral almacenada contiene:
- Metadatos comunes (capa, color, handle).
- `recipe: Recipe`: Árbol de operaciones paramétricas de construcción (`FeatureNode`: caja, cilindro, extrusión, booleanas, etc.).
- `bbox: Bounds3`: Caja envolvente mínima para consultas de selección y encuadre rápido sin reevaluar.
- `snapshot: Option<BrepSnapshot>`: Instantánea binaria opcional para acelerar la carga en frío.
- `mesh_cache: Option<TriangleMesh>`: Malla teselada guardada opcionalmente para dibujo rápido.

`cadcraft-doc` **no dependerá de `truck`**. Solo almacenará estructuras declarativas libres de tipos externos.

### 4.2. Evaluación mediante `cadcraft-brep`
`cadcraft-engine` orquesta la evaluación:
1. Si existe un `snapshot` compatible (`snapshot.validate_compatibility().is_ok()`), carga `BrepSolid::from_snapshot(snapshot)` directamente en $\approx 0.5 \ \text{ms}$.
2. Si el snapshot no existe o es incompatible (versión de kernel distinta), `cadcraft-engine` invoca `recipe.evaluate()`, que reconstruye el sólido B-Rep exacto ejecutando las primitivas y operaciones booleanas.

### 4.3. Datos Volátiles vs. Conservados entre Sesiones
- **Conservado entre sesiones**:
  - `Recipe` (fuente de verdad canónica y neutral).
  - Bounding box 3D y propiedades de estilo.
  - Snapshot versionado (si se opta por guardarlo).
- **Caché en memoria (volátil)**:
  - El objeto `BrepSolid` en memoria con estructuras de datos vivas de Truck.
  - Buffers de vértices e índices GPU en `cadcraft-render` (WGPU).

### 4.4. Versionado y Migración de Formatos
- El registro de persistencia incluye `schema_version: u32` (versión del esquema CADCraft) y `kernel_revision: String`.
- Si CADCraft actualiza el kernel en el futuro (ej. a una nueva revisión Git o versión mayor), las recetas paramétricas son 100% compatibles hacia adelante. El snapshot viejo simplemente se invalida y la receta se reevalúa de forma transparente.

### 4.5. Tratamiento de Fallos en Geometría 3D (Never Crash)
Si un dibujo contiene una receta que no puede evaluarse con el kernel actual (por ejemplo, una operación booleana que devuelve error):
- **Cero pérdida de datos**: La entidad permanece en el dibujo con su `Recipe` intacta; el dibujo abre con éxito.
- **Estado de error no bloqueante**: La entidad almacena `evaluation_status: Err("...")`.
- **Representación visual de degradación segura**: Si existe una malla proxy previa guardada, se renderiza con un contorno de advertencia; si no existe, se dibuja su caja envolvente en modo alámbrico punteado.
- El usuario o JARVIS pueden inspeccionar el error mediante `query.entity` y ajustar los parámetros de la operación para corregirla.
