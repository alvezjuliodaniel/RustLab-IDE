# Hoja de ruta de RustLab IDE

Cada fase se cierra con los resultados de [criterios de aceptación](CRITERIOS_ACEPTACION.md) registrados en [ESTADO.md](ESTADO.md). Ejecutar pruebas tras cada cambio significativo. Cada incremento de código debe pasar `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` y `cargo test --workspace --locked`. Registrar comandos, plataforma y evidencia de las pruebas manuales.

| Fase | Tareas verificables | Cierre |
|---|---|---|
| 0 · Auditoría y diseño | Inventariar workspace y herramientas. Verificar versiones y fuentes. Diseñar contratos y redactar cinco entregables. | CA-00 |
| 1 · Aplicación mínima | Inicializar Git y workspace. Preparar dependencias GTK y validar compilación, enlace y apertura. Crear menús, acciones, explorador de carpetas, editor con pestañas y panel inferior. Implementar feedback, guardado y navegación de teclado. | CA-01 |
| 2 · Proyectos Cargo | Implementar nuevo/abrir/importar, plantillas de consola, biblioteca, workspace, GUI y embebido. Cargar metadatos y targets, administrar archivos/módulos, mover y duplicar. Añadir recientes, gestión de manifiesto y restauración por proyecto. | CA-02, CA-12 |
| 3 · Compilación y ejecución | Supervisar procesos en segundo plano, captura y cancelación. Integrar check, build, run, test, clean, doc, clippy y fmt. Configurar perfiles, target, binario o ejemplo, features, argumentos y entorno. Vincular diagnósticos y artefactos a la UI. | CA-03 a CA-06 |
| 4 · Editor inteligente | Instalar y verificar rust-analyzer estable. Implementar transporte, ciclo de vida y pruebas de sincronización. Añadir completado, parámetros, documentación, navegación, referencias, renombrado, acciones y diagnósticos en vivo. Completar búsqueda global, símbolos y plegado tras validar el modelo de rangos. | CA-07 |
| 5 · Depuración | Probar `lldb-dap-19` con un ejecutable Rust real. Integrar configuración, breakpoints, control de ejecución, consola e inspección. Validar capacidades opcionales y cierre de sesión. | CA-08, CA-09 |
| 6 · Herramientas profesionales | Implementar dependencias y conflictos, Git, pruebas y benchmarks, cobertura opcional, toolchains y targets. Completar configuración avanzada, atajos, paneles configurables, múltiples ventanas y proyectos. | CA-10, CA-12 |
| 7 · Experiencia educativa | Incluir ayuda sin red, tutoriales, ejercicios, casos de errores y documentación contextual. Implementar modos simple y profesional y definir interfaz opcional para asistentes. | CA-11 |
| 8 · Extensiones y distribución | Definir manifiesto, API, permisos y aislamiento. Validar paquetes en sistemas limpios, Linux ARM64 y Windows x86-64. Registrar plataformas efectivamente soportadas. | CA-13 y regresión completa |

## Primer incremento propuesto para la Fase 1

1. Verificar `pkg-config` para GTK4, libadwaita y GtkSourceView en Debian 13.
2. Compilar y abrir una ventana con un buffer Rust real para confirmar bindings, enlace y display.
3. Completar el flujo de abrir carpeta, editar, guardar y validar CA-01 antes de pasar a proyectos Cargo.

## Decisiones que se validan durante el desarrollo

- Fase 1. Versión mínima de Rust del conjunto resuelto, herramienta de UI para pruebas automatizadas y virtualización del árbol en carpetas grandes.
- Fase 2. Plantillas reproducibles para GUI y embebido con requisitos visibles antes de crear. Definir targets embebidos de referencia.
- Fase 4. Validar plegado y múltiples cursores con GtkSourceView. Registrar limitaciones y cobertura si la prueba técnica impide cumplir esa capacidad.
- Fase 5. Validar lanzamiento de procesos y formatos de variables Rust en LLDB 19 con los fixtures de depuración.
- Fase 8. Validar disponibilidad de la pila gráfica por plataforma, formato de paquetes y aislamiento antes de habilitar extensiones de terceros.
