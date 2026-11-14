# Arquitectura de RustLab IDE

Propuesta de Fase 0 · 14 de noviembre de 2026

## Plataforma y dependencias

Debian 13 x86-64 es el primer entorno soportado. Usar Rust estable, edición 2024 y un workspace Cargo con dependencias compartidas y `Cargo.lock` versionado. Registrar la versión mínima de Rust tras compilar y probar el conjunto completo de dependencias en la Fase 1.

| Componente | Base propuesta | Evidencia |
|---|---|---|
| GTK4 | `gtk4 0.10.3`, API hasta 4.18 | APT ofrece GTK 4.18.6 |
| libadwaita | `libadwaita 0.8.0`, API hasta 1.7 | APT ofrece libadwaita 1.7.6 |
| GtkSourceView | `sourceview5 0.10.0`, API hasta 5.16 | APT ofrece GtkSourceView 5.16.0 |
| Análisis | rust-analyzer de la toolchain, LSP 3.17 según capacidades | El componente estable requiere instalación |
| Depuración | `lldb-dap-19` por stdio, ruta configurable | Debian lo distribuye en `lldb-19` |
| Control de versiones | CLI Git y mecanismos de credenciales del entorno | Git 2.47.3 disponible |

Los manifiestos publicados de libadwaita y sourceview5 comparten la serie GTK 0.10. Las versiones y límites de API son una base para el primer ensayo de compilación y enlace. Habilitar únicamente las APIs usadas. [GTK 0.10.3](https://crates.io/crates/gtk4/0.10.3) · [libadwaita 0.8.0](https://crates.io/crates/libadwaita/0.8.0) · [SourceView 0.10.0](https://crates.io/crates/sourceview5/0.10.0) · [Archivos de lldb-19](https://packages.debian.org/trixie/amd64/lldb-19/filelist).

## Workspace y direcciones de dependencia

Crear crates al incorporar su primera función operativa. Los nombres abreviados de la tabla llevan el prefijo `rustlab-`.

| Crate | Responsabilidad y dependencias internas permitidas |
|---|---|
| app | Composición, arranque, coordinación por ventana y servicios. Puede importar todos los crates. |
| core | Tipos compartidos, errores y contratos de procesos y ubicaciones. Sin dependencias internas. |
| ui | Ventana, explorador, acciones y paneles GTK. Depende de editor y core. |
| editor | GtkSourceView, pestañas, buffers, búsqueda, revisión y traducción de posiciones. Depende de core. |
| projects | Descubrimiento, árbol de archivos, plantillas y operaciones autorizadas. Depende de cargo y core. |
| cargo | Metadatos, operaciones Cargo, diagnóstico y artefactos. Depende de core. |
| lsp | Cliente y supervisor de rust-analyzer. Depende de core. |
| debug | Cliente DAP, sesiones y estado de depuración. Depende de core. |
| git | Estado, diffs y operaciones Git. Depende de core. |
| help | Catálogo, tutoriales, ejemplos y acceso a documentación local. Depende de core. |
| settings | Configuración, migraciones y restauración. Depende de core. |
| extensions | Manifiestos, permisos y ejecución aislada cuando se valide ese mecanismo. Depende de core. |

`tests/` contendrá proyectos de integración, `examples/` ejemplos para usuarios, `docs/` documentación adicional y `assets/` recursos de interfaz. Ningún servicio de dominio manipula widgets. Los tipos propios de GTK permanecen en app, ui y editor. La UI emite acciones y app coordina resultados y ciclo de vida.

## Contratos entre módulos

| Contrato conceptual | Entrada | Salida y ciclo de vida |
|---|---|---|
| Proyecto | Carpeta o manifiesto seleccionado | Identidad del workspace, miembros, targets y rutas mediante `cargo metadata --format-version 1 --no-deps`. Una carpeta común se puede explorar con acciones Cargo deshabilitadas. |
| Documento | Ruta, bytes y acciones de edición | Buffer con revisión, estado de modificaciones y huella de disco. Conserva UTF-8, BOM si existe y saltos LF o CRLF. Codificaciones sin soporte requieren una elección explícita antes de editar. |
| Operación Cargo | Workspace, manifiesto, target, perfil, features, argumentos y entorno | Eventos identificados de inicio, salida, diagnóstico, artefacto y resultado final con duración. Los ejecutables provienen del campo `executable` en `compiler-artifact`. |
| LSP | Raíz, documentos versionados y solicitudes del editor | Diagnósticos y resultados ligados a sesión y revisión. Conversión de posiciones según codificación negociada, incluido UTF-16 por defecto. |
| DAP | Ruta exacta del ejecutable, argumentos, entorno y breakpoints | Estado de sesión, capacidades, salida, threads, frames y variables. Las referencias de inspección se invalidan al continuar. |
| Configuración | Preferencias globales, ajustes por proyecto y estado de ventana | Esquema versionado. Las preferencias por proyecto prevalecen sobre las globales. Los datos residen inicialmente en directorios XDG, asociados al workspace. |

Usar identificadores independientes para documento, operación y sesión. Cerrar una pestaña, cambiar de proyecto o arrancar una nueva sesión invalida los resultados anteriores que ya no correspondan. El editor aplica cambios LSP a partir de una revisión comprobada y muestra una vista previa cuando se afectan varios archivos.

## Procesos y protocolos

GTK se actualiza en el hilo principal. Escaneo, E/S y procesos externos se ejecutan fuera de ese hilo. Los lectores drenan stdout y stderr en paralelo. Agrupar actualizaciones y acotar la salida retenida para que una compilación intensa no monopolice la interfaz. Usar argumentos separados y directorio de trabajo explícito. En Linux, arrancar tareas cancelables en grupos de procesos propios, señalizar el grupo, escalar tras un plazo y recoger al hijo. Abstraer el mecanismo para evaluar Job Objects en Windows.

Consumir el JSON de Cargo por líneas y conservar texto ordinario, incluido el de ejecutables lanzados por `cargo run`. Registrar el estado de salida real del proceso. Relacionar diagnósticos con su origen y snapshot. Si un archivo cambió, informar que la ubicación puede haber quedado desactualizada. [Cargo Book](https://doc.rust-lang.org/cargo/reference/external-tools.html).

LSP usa JSON-RPC con `Content-Length`. Negociar capacidades en `initialize`, enviar `initialized` y respetar el modo de sincronización con `didOpen`, `didChange`, `didSave` y `didClose` según corresponda. Implementar cancelación y tiempos límite, descartar respuestas obsoletas y reabrir documentos activos tras un reinicio acotado. Para cierre normal, usar `shutdown` y `exit`. [LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) · [rust-analyzer](https://rust-analyzer.github.io/book/).

DAP correlaciona peticiones y respuestas con secuencias. Inicializar, enviar `launch` y atender el evento `initialized` para configurar breakpoints antes de `configurationDone` cuando se anuncie soporte. Actualizar la ubicación y verificación que devuelve el adaptador. Habilitar funcionalidades opcionales según capacidades y estado. La depuración comienza con un artefacto seleccionado que tenga símbolos. [DAP](https://microsoft.github.io/debug-adapter-protocol/specification) · [LLVM LLDB DAP](https://lldb.llvm.org/use/lldbdap.html).

## Escrituras e internacionalización

Guardar al accionar Guardar o confirmar un cambio, verificando la versión en disco y usando reemplazo atómico cuando sea compatible con el archivo y el sistema. Si hubo un cambio externo, ofrecer recargar, guardar copia o confirmar la sobrescritura. Presentar operaciones de escritura que alcancen otros archivos, como formateo de proyecto, renombrado y cambios de dependencias. Explicar que Cargo y rust-analyzer pueden ejecutar scripts de compilación o macros al habilitar herramientas para un proyecto externo. Usar mecanismos de Git para las credenciales y excluir secretos de logs.

Mantener cadenas traducibles desde la primera ventana, con catálogo inicial en español, texto dinámico parametrizado y convenciones de pluralización. Aislar rutas, procesos y almacenamiento de las APIs GTK para validar Linux ARM64 y Windows x86-64 en la Fase 8.
