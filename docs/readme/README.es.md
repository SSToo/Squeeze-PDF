# Squeeze PDF

[English](../../README.md) · [简体中文](README.zh-CN.md) · [Français](README.fr.md) · **Español** · [한국어](README.ko.md) · [日本語](README.ja.md) · [Deutsch](README.de.md) · [Português](README.pt.md)

Compresor de PDF de código abierto y sin conexión para Windows y macOS, actualmente en fase MVP. La interfaz usa Tauri 2 y React. Rust implementa las estrategias de compresión, el tratamiento de imágenes, la reconstrucción de páginas y la protección de archivos. lopdf lee y escribe PDF; el escritorio renderiza con PDF.js local y la CLI usa hayro por defecto. MozJPEG se enlaza estáticamente mediante Rust. No requiere qpdf, Ghostscript ni Python durante la ejecución.

Descarga los instaladores para Windows x64, macOS Apple silicon y macOS Intel desde [GitHub Releases](https://github.com/SSToo/Squeeze-PDF/releases). Al enviar una etiqueta `v` que coincida con la versión de la aplicación, los instaladores y sus sumas de verificación se publican automáticamente cuando terminan correctamente las tres compilaciones. Consulta la [guía de publicación](../RELEASING.md) (inglés).

Desde 0.1.25, las versiones publicadas buscan actualizaciones estables en GitHub unos cinco segundos después del inicio y cada 24 horas. El icono de actualización permite comprobarlo manualmente. Tú eliges instalar y reiniciar, una vez finalizadas las tareas de archivos. Los fallos de conexión no interrumpen la compresión. La red solo se usa para consultar y descargar actualizaciones; los PDF nunca se suben. Las versiones 0.1.24 y anteriores necesitan primero una actualización manual.

## Licencia

El código propio del proyecto usa [Apache-2.0](../../LICENSE), con atribución en [NOTICE](../../NOTICE). Permite uso comercial y productos derivados de código cerrado, sujetos a conservar licencias y avisos, identificar modificaciones y cumplir las demás condiciones. El texto de la licencia rige la concesión de patentes y sus condiciones.

Las bibliotecas, fuentes y recursos de terceros conservan sus licencias. Los paquetes incluyen `third-party/PROJECT-LICENSE.txt`, `PROJECT-NOTICE.txt`, avisos de terceros y los códigos fuente requeridos. Consulta [Contribuciones](../../CONTRIBUTING.md) y el [registro de preparación para código abierto](../OPEN-SOURCE.md).

## Dos modos de salida

| Modo                         | Resultado                                                                                | Uso habitual                                                            |
| ---------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Conservar contenido original | Mantiene texto, fuentes y vectores; optimiza trazados, estructura e imágenes compatibles | Informes, contratos, planos, texto seleccionable y ampliación           |
| Páginas como imágenes        | Renderiza cada página como una imagen independiente y reconstruye el PDF                 | Páginas uniformes cuando no se necesita texto seleccionable ni vectores |

El modo y la calidad se eligen por separado:

| Calidad                       | Contenido original                                                                   | Páginas como imágenes                              |
| ----------------------------- | ------------------------------------------------------------------------------------ | -------------------------------------------------- |
| Alta fidelidad / Alta calidad | Resolución original, JPEG 95, optimización de precisión y reutilización de contornos | 300 DPI, píxeles codificados sin pérdida con Flate |
| Equilibrado                   | Optimización de trazados, JPEG 85, hasta 200 DPI                                     | Ancho de 1400 px, calidad JPEG 72                  |
| Más pequeño                   | Optimización de trazados, JPEG 50, hasta 100 DPI                                     | Ancho de 1000 px, calidad JPEG 60                  |

Alta fidelidad conserva las dimensiones, pero puede modificar los píxeles: **no es estrictamente sin pérdida**. El redondeo vectorial limita el error a unos 0,0005 puntos por eje bajo las transformaciones conocidas. No redondea la posición del texto ni los colores; los bordes muy finos aún pueden cambiar. Los contornos cerrados compatibles con relleno sólido se reutilizan como Forms vectoriales.

Alta calidad codifica sin pérdida los píxeles ya renderizados; texto y vectores ya están rasterizados. El modo imagen elimina texto seleccionable, trazados, enlaces y marcadores. En escritorio, formularios y anotaciones visibles que pueden renderizarse se integran en las imágenes; no se conservan campos ocultos ni textos emergentes de anotaciones. Se mantienen los datos de archivos adjuntos incrustados.

Conservar contenido solo exporta un PDF validado más pequeño que el original. Compara candidatos como archivos completos y revierte si aumentan. El modo imagen respeta el formato pedido aunque crezca el archivo e indica el aumento. Los originales son de solo lectura, las salidas existentes no se sobrescriben y un cambio en la entrada impide confirmar la salida.

El motor compara JPEG, Flate exacto, paletas Indexed, 1 bit y CCITT G4 cuando son compatibles. También optimiza coeficientes JPEG, recursos compartidos, subconjuntos CID limitados y disposición del archivo. El escritorio puede comparar renderizado directo y renderizado 2× reducido con presupuestos y umbrales de calidad. Consulta los [límites de implementación y validación](../VALIDATION.md).

## Funciones de escritorio

- Arrastrar o seleccionar varios PDF, eliminar duplicados y procesarlos en serie.
- Dos modos, tres calidades, etapas de proceso y contador de archivos terminados.
- Tamaño antes/después, ahorro o aumento, detalles y errores en la fila del archivo.
- Vista previa original/resultado en paralelo, páginas y zoom; recursos completamente locales.
- Carpeta de salida, numeración automática, cancelación, reintento y apertura del resultado o su carpeta.
- Recuerda modo, calidad, carpeta y tema; la cola solo dura la sesión actual.
- El icono a la derecha del tema abre inglés, chino simplificado, francés, español, coreano, japonés, alemán y portugués. Al inicio usa el idioma compatible del sistema o inglés; luego recuerda la elección.
- Interfaz, etapas, errores y vista previa cambian sin interrumpir tareas. Nombres, rutas y errores técnicos desconocidos mantienen su valor original.
- Logo transparente, acentos rojos y tema oscuro gris carbón.

## Desarrollo

Requiere Node.js 22.13+, Rust 1.92.0 fijado en `rust-toolchain.toml` y un compilador C de la plataforma. macOS necesita Xcode Command Line Tools; Windows, MSVC C++ Build Tools y WebView2. MozJPEG se compila dentro del worker; SIMD en ensamblador está desactivado, sin requerir NASM ni CMake.

Ejecuta desde la raíz del repositorio:

```sh
npm ci
npm run tauri -- dev
```

`npm run preview` inicia solo la interfaz; las operaciones nativas requieren `tauri dev`. Los scripts preparan worker, PDF.js local y licencias. `private: true` evita publicar accidentalmente en npm y no cambia la licencia de código abierto.

## Comprobaciones y paquetes

```sh
cargo test --workspace --locked
node scripts/prepare-worker.mjs
npm run notices
npm run test:licenses
npm run check
npm run test:render
npm run test:i18n
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# Aplicación macOS
npm run tauri -- build --bundles app
# Aplicación macOS y DMG sencillo
npm run package:mac
# Instalador EXE; ejecutar en Windows
npm run tauri -- build --bundles nsis
```

Paquetes en `target/release/bundle/`, para la arquitectura del equipo. Al usar `--target`, define también `CARGO_BUILD_TARGET` para que coincida el worker. El instalador Windows incluye WebView2 sin conexión.

La CI está configurada para Windows x64, macOS arm64 e Intel. La validación local se hizo en macOS arm64; faltan pruebas de instalación Windows/Intel. macOS usa firma ad-hoc para pruebas locales. Firma de producción Windows, Developer ID macOS y notarización aún no están configurados. Para publicar, define `APPLE_SIGNING_IDENTITY` y sigue la [documentación Tauri](https://v2.tauri.app/distribute/sign/macos/).

## CLI y motor

```sh
cargo run -p squeeze-cli -- inspect "input.pdf"
cargo run -p squeeze-cli -- compress "input.pdf" -o "fidelity.pdf" --output-mode preserve --preset fidelity --compact-paths --reuse-paths --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "balanced.pdf" --output-mode preserve --preset balanced --json
cargo run -p squeeze-cli -- compress "input.pdf" -o "raster.pdf" --output-mode raster --preset small --json
```

`--output-mode`: `preserve` por defecto o `raster`. Calidades: `lossless`, `fidelity`, `balanced`, `small`; las compatibles admiten `--quality 85 --dpi 200`. `--compact-paths` y `--reuse-paths` habilitan precisión y reutilización de contornos; desactivados por defecto en CLI, activados en las tres calidades de conservación del escritorio.

`lossless` no recuantiza ni reduce imágenes e ignora esas dos opciones vectoriales; permite codificaciones exactas, optimización JPEG de coeficientes y recursos equivalentes compartidos. `fidelity` conserva dimensiones, usa JPEG 95 y no acepta DPI objetivo. Alta calidad usa codificación de píxeles sin pérdida. Ctrl+C solicita cancelar. En escritorio se detiene el renderizado o el worker independiente. Renderizado de páginas y codificación del worker tienen cada uno un límite de 120 segundos.

API: `squeeze_core::inspect_file`, `squeeze_core::audit_file`, `squeeze_core::compress`. `squeeze audit input.pdf` proporciona un desglose de tamaño de solo lectura. El JSON informa tamaños, etapas, candidatos, tiempos, caché y selección de escritura. Los bytes de flujos/candidatos no representan el PDF completo ni implican redundancia eliminable.

## Compatibilidad y límites

- Sin límite fijo de tamaño PDF; más de 256 MiB según memoria disponible. Flujo descomprimido hasta 64 MiB e imagen hasta 24 millones de píxeles; ejecución en serie.
- Optimiza RGB/Gray y entradas limitadas ICCBased, Indexed, G4 y componentes de color sin procesar. Conserva prudentemente codificaciones/mapeos desconocidos; ciertos JPEG CMYK/YCCK admiten optimización de coeficientes sin recodificar píxeles. Protege dimensiones y semántica de color/Decode/máscaras cuando corresponde; no admite todos los códecs.
- Escala desconocida o análisis incompleto desactiva la reducción de imágenes afectadas. Alta fidelidad siempre conserva dimensiones originales.
- Conserva formularios, anotaciones y adjuntos, protegiendo apariencias e imágenes compartidas. Las imágenes ordinarias independientes siguen siendo optimizables. Formularios/anotaciones con apariencia desactivan prudentemente las dos optimizaciones vectoriales con pérdida. Se omiten PDF cifrados, firmados digitalmente, PDF/A, XFA y portfolios.
- El escritorio acepta formularios/anotaciones renderizables, enlaces ordinarios, contenedores vacíos y adjuntos. Apariencias ausentes o anotaciones dañadas causan error por página. hayro CLI rechaza formularios/anotaciones que deben dibujarse y flujos `BrotliDecode`; usa PDF.js en escritorio.
- Maneja rotación, recorte visible y tamaño físico. Rechaza la conversión si faltan fuentes no estándar incrustadas. Revisa páginas complejas; hayro evoluciona y no garantiza compatibilidad total con PDF comerciales.
- El analizador puede reparar referencias cruzadas; no rechaza estrictamente todos los PDF malformados. Los límites existentes no cubren toda asignación de memoria.
- Cancelar normalmente limpia temporales del worker detenido. No hay recuperación automática tras cierre forzado o corte de energía.
- No incluye OCR, editor PDF, tamaño final garantizado ni activación comercial.

## Pruebas y registros técnicos

```sh
node scripts/benchmark.mjs
node scripts/benchmark-round1.mjs tmp/round1-benchmark /path/to/baseline-worker
cargo run -p squeeze-core --example make_sample -- tmp/samples --large
```

La muestra grande añade un PDF de 24 páginas y unos 138 MB para cancelación y archivos grandes. Las tasas sintéticas no predicen documentos reales. Poppler, pypdf, Pillow y NumPy se usan en verificaciones independientes de desarrollo, no durante la ejecución del producto.

Registros detallados en inglés: [validación](../VALIDATION.md), [plan](../../PLAN.md). Jpegli y las salidas Brotli son experimentos, no dependencias ni opciones de producción.

## Materiales de terceros

Los archivos de bloqueo fijan dependencias. `npm run notices` recopila licencias, NOTICE de hayro, avisos de fuentes/WASM PDF.js, MIT QuickJS/Mozilla, fuentes MPL sin modificar y fuentes Liberation 1.07.4, además de TTX editables de las fuentes distribuidas. Se incluyen en `third-party/`; el inventario incluye prudentemente dependencias de compilación. Consulta [obtención y reconstrucción](../../licenses/SOURCE-CODE.md) y el [registro de licencias](../LICENSING-0.1.22.md).

`npm run test:licenses` verifica textos, hashes e integridad MPL en CI Windows/macOS. Cambiar PDF.js o recursos requiere revisar licencias y fuentes. `scripts/make-font-sources.py` necesita FontTools 4.66.1 solo para desarrollo. Liberation conserva GPL con excepciones de fuentes; los demás componentes mantienen sus licencias.

PDF: [lopdf](https://docs.rs/lopdf/0.45.0/lopdf/); renderizado: [PDF.js](https://mozilla.github.io/pdf.js/getting_started/) o [hayro](https://github.com/LaurenzV/hayro); decodificación: image; JPEG: MozJPEG; G4: `fax 0.3.0` bajo MIT; subconjuntos CID TrueType limitados: `subsetter 0.2.6` bajo MIT/Apache-2.0. No se distribuyen fuentes de prueba. El código propio usa Apache-2.0 desde 0.1.23; no cambia las licencias de terceros.
