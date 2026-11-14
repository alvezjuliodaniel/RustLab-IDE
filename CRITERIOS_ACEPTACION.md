# Criterios de aceptación

La Fase 0 entrega evidencia del entorno y un plan de verificación. Las pruebas de producto se ejecutan en la fase asignada y la regresión completa se repite antes de distribuir. Guardar resultado, pasos, versiones y log o captura para cada prueba manual. Automatizar las comprobaciones con salida determinista.

## Proyectos de prueba

Crear en las fases correspondientes y ejecutar en copias temporales. La aplicación debe operar sobre proyectos Cargo válidos.

- **Consola.** Binario sin dependencias con argumento y variable de entorno, una prueba unitaria y una de integración. Variante con error de tipo en una línea conocida.
- **Workspace mixto.** Biblioteca, dos binarios, ejemplo, feature opcional y ruta de artefactos personalizada. Usar rutas con espacios y archivos UTF-8 con caracteres fuera del plano básico y CRLF.
- **Depuración.** Binario Debug con una función llamada desde `main`, variable local de valor conocido y un segundo thread. Usar argumentos y entorno de prueba.
- **Git local.** Repositorio con cambios preparados y sin preparar, dos ramas, conflicto reproducible y remoto bare para push/pull.
- **Fallos controlados.** Tool ausente, cancelación con proceso descendiente, stdout voluminoso, archivo cambiado externamente y salida inesperada del servidor LSP y DAP.

## Verificación por fase

| ID | Fase | Prueba y resultado observable |
|---|---|---|
| CA-00 | 0 | Los cinco documentos existen, la auditoría distingue pruebas locales de versiones consultadas y cada fase tiene una condición de cierre. |
| CA-01 | 1 | Abrir el IDE en Debian 13 desde una instalación de desarrollo. Abrir carpeta, seleccionar archivo Rust y comprobar pestaña, resaltado y números. Editar y guardar, verificar bytes en disco y comprobar Guardar, Descartar y Cancelar al cerrar. Probar menús, barra de herramientas, foco y panel inferior con estados vacíos y de error. |
| CA-02 | 2 | Crear desde la GUI consola y biblioteca y abrir el workspace mixto existente. Cargo lee los manifiestos generados y el explorador identifica miembros, bins y ejemplos. Probar plantillas restantes con sus toolchains indicadas. Crear, mover y renombrar archivos/módulos, duplicar y comprobar recientes y cambios de manifiesto. |
| CA-03 | 3 | Ejecutar check, build y run desde la UI. Configurar el segundo binario, un ejemplo, features, entorno y argumentos. La salida coincide con lo esperado, incluye ambos streams, duración y estado. Verificar además test, clean, doc, clippy y fmt en el fixture. |
| CA-04 | 3 | Introducir un error de tipo, ejecutar Verificar y abrir su diagnóstico. El cursor llega a la ubicación esperada. Corregir, volver a verificar y comprobar la lista de diagnósticos actualizada. |
| CA-05 | 3 | Compilar Release con ruta de artefactos personalizada. Seleccionar el ejecutable anunciado por Cargo, abrir su carpeta y comprobar que el archivo ejecuta. Confirmar que biblioteca y binarios distintos no se confunden. |
| CA-06 | 3 | Cancelar una tarea con un descendiente y salida intensa. La UI sigue reaccionando a teclado y edición, se confirma el fin de la tarea y su descendiente y se puede iniciar una nueva tarea. Repetir con un fallo de arranque. |
| CA-07 | 4 | Obtener un completado semántico de un símbolo de otro crate, navegar a declaración y referencias, ver parámetros y hover, aplicar acción rápida y renombrar. Comprobar diagnóstico en vivo, formato, búsqueda y plegado. Repetir con UTF-16, respuestas fuera de orden, cancelación y reinicio del servidor con un buffer editado. |
| CA-08 | 5 | Seleccionar el binario Debug, establecer un breakpoint y comprobar su verificación. Lanzar, detenerse en la línea, entrar/avanzar/salir y comprobar archivo y línea. Probar continuar, pausar, reiniciar y detener. |
| CA-09 | 5 | En la pausa de CA-08, confirmar el valor local conocido, evaluar una expresión y navegar stack y threads. Probar condiciones, memoria y registros en adaptadores que los anuncien. Simular salida del adaptador y confirmar que se libera la sesión y se puede reiniciar. |
| CA-10 | 6 | Agregar, actualizar y quitar una dependencia en un fixture. Validar árbol, features y conflicto con Cargo. Probar estado, diff, stage, unstage, commit, ramas, checkout, merge y conflicto Git y sincronizar contra remoto local. Ejecutar una prueba, un módulo y el conjunto completo, y comprobar resultados y fallos. Validar opciones de benchmark, cobertura, toolchains y targets de acuerdo con herramientas habilitadas. |
| CA-11 | 7 | Con red deshabilitada y documentación local instalada, abrir ayuda en español, completar un ejercicio, ejecutar su ejemplo y consultar una explicación de compilador. Alternar modos y comprobar que se llega a funciones avanzadas desde el modo profesional. |
| CA-12 | 2 y 6 | Cambiar configuración de un proyecto, cerrar y reabrir. Mantener proyecto, pestañas y perfil en Fase 2. Repetir con dos ventanas, varios proyectos, paneles y atajos en Fase 6. Forzar una escritura fallida y comprobar que se conserva la configuración previa. |
| CA-13 | 8 | Instalar y arrancar en sistemas limpios de cada plataforma objetivo. Ejecutar regresión de consola y depuración para las plataformas soportadas. Validar rechazo de manifiesto incompatible y comprobar que una extensión de prueba ve solo recursos autorizados. |

Verificar además que un archivo cambiado externamente requiera una decisión de guardado y que las operaciones canceladas o fallidas dejen disponibles los controles correspondientes.
