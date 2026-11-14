# Especificación funcional de RustLab IDE

## Interfaz y navegación

La ventana reúne los menús Archivo, Editar, Ver, Proyecto, Compilar, Ejecutar, Depurar, Herramientas y Ayuda. La barra superior ofrece Verificar, Compilar, Ejecutar, Depurar y Detener. El explorador lateral muestra carpetas y crates. El editor central usa pestañas y el panel inferior reúne Salida, Problemas, Consola, Pruebas y Depuración. Un panel contextual presenta documentación y la barra de estado muestra proyecto, posición, perfil y operación activa.

Los paneles se pueden redimensionar y ocultar. La interfaz inicia en español y ofrece temas claro y oscuro, foco visible, navegación por teclado y atajos configurables. Las acciones deshabilitadas muestran el motivo. El modo simple reduce los controles visibles y el modo profesional expone configuración avanzada. Cada ventana conserva su contexto al abrir varios proyectos.

## Edición y proyectos

El usuario puede abrir carpetas y archivos, crear e importar proyectos Cargo y navegar workspaces con varios crates. Puede crear aplicaciones de consola, bibliotecas, workspaces y proyectos GUI o embebidos con plantillas. La creación valida nombre, carpeta, toolchain y requisitos de plantilla. La gestión incluye archivos, módulos, movimientos, renombrado, duplicación, perfiles, proyectos recientes, búsqueda de archivos y símbolos y edición de Cargo.toml. Abrir un proyecto existente conserva su estructura.

Cada pestaña muestra estado de guardado, números de línea y resaltado Rust. El editor ofrece deshacer y rehacer, búsqueda y reemplazo, búsqueda global, plegado y múltiples cursores donde la prueba técnica lo permita. Se preservan codificación y saltos de línea soportados. Los cambios externos generan una decisión antes de reemplazar ediciones propias. Al cerrar con cambios, el usuario puede guardar, descartar o cancelar.

Con rust-analyzer activo, el usuario obtiene autocompletado semántico, información de parámetros, documentación emergente, definición, referencias, renombrado, acciones rápidas y diagnósticos en vivo. Puede formatear manualmente o al guardar si habilitó esa preferencia. Si el servidor deja de responder, la edición básica sigue disponible y la barra de estado informa la recuperación.

## Compilación, ejecución y depuración

Antes de iniciar una operación Cargo, se puede elegir perfil Debug o Release, crate, binario o ejemplo, target, features y, para ejecución, argumentos y entorno. Se ofrecen check, build, run, test, clean, doc, clippy y fmt. La salida muestra progreso, duración y resultado, con stdout y stderr. Los diagnósticos abren el archivo y rango asociado. Los artefactos ejecutables de una compilación se pueden seleccionar y localizar en su carpeta.

Detener cancela la operación activa seleccionada y la vista refleja el resultado. El flujo de depuración ofrece selección de ejecutable y configuración de ejecución, edición de breakpoints, continuar, pausar, entrar, avanzar, salir, reiniciar y detener. Presenta variables, expresiones observadas, stack, threads y consola. Los breakpoints condicionales, memoria y registros se presentan según las capacidades disponibles.

## Dependencias, Git y calidad

La vista de Cargo presenta dependencias, versiones, features, metadatos disponibles, árbol y conflictos. Permite agregar, eliminar y actualizar, con confirmación en cambios destructivos. La vista Git usa el repositorio existente para estado, diff, stage, unstage, commit, historial, ramas, checkout, merge, conflictos, push y pull.

El panel Pruebas descubre y ejecuta pruebas por nombre y módulo y registra resultados. La interfaz integra Clippy, rustfmt, benchmarks compatibles y cobertura con herramientas opcionales. Los diagnósticos permiten navegar al archivo cuando incluyen ubicación.

## Ayuda, configuración y extensiones

Primeros pasos, tutoriales y ejercicios en español abarcan Cargo, ownership, borrowing, tipos y traits. Incluyen ejemplos ejecutables y explicaciones de errores mediante documentación integrada o local disponible. Las funciones de aprendizaje local operan sin red ni asistentes de IA.

Las preferencias globales y por proyecto abarcan editor, tema, atajos, toolchains y targets de rustup, compilación y depuración. Al reabrir se restauran el proyecto, pestañas, layout y perfiles guardados. Los fallos se informan en la interfaz y dejan un registro estructurado. El sistema de extensiones expone versión de API, permisos e instalación controlada mediante manifiestos y aislamiento.
