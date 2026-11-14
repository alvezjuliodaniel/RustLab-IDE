# Estado de RustLab IDE

**14 de noviembre de 2026 · Fase 0 completada. Fase 1 operativa con verificación parcial de CA-01. Fase 2 verificada en Debian para los flujos incluidos. Fase 3 en desarrollo.**

## Auditoría del entorno

El directorio de trabajo estaba vacío y Git se inicializó durante el desarrollo.

| Elemento | Resultado observado |
|---|---|
| Sistema | Debian GNU/Linux 13.7 (trixie), x86_64. `DISPLAY=:0` presente, apertura de ventana pendiente. |
| Rust | Toolchain estable activa `stable-x86_64-unknown-linux-gnu`, rustc y Cargo 1.99.1 reportados localmente. Dos toolchains nightly instaladas. |
| Componentes | rustfmt 1.10.0-stable, Clippy 0.1.99, rust-src y documentación Rust instalados. Target estable disponible `x86_64-unknown-linux-gnu`. |
| rust-analyzer | Proxy presente en PATH. `rust-analyzer --version` falla porque falta el binario de la toolchain estable. |
| Utilidades | Git 2.47.3, GCC 14.2.0, pkg-config 1.8.1. |
| GTK4 | Sin paquete de desarrollo ni entrada en pkg-config. Candidato APT `libgtk-4-dev 4.18.6+ds-2`. |
| libadwaita | Sin paquete de desarrollo ni entrada en pkg-config. Candidato APT `libadwaita-1-dev 1.7.6-1~deb13u1`. |
| GtkSourceView | Sin paquete de desarrollo ni entrada en pkg-config. Candidato APT `libgtksourceview-5-dev 5.16.0-1`. |
| LLDB y DAP | Paquetes LLDB sin instalar. Candidato APT `lldb-19 1:19.1.7-3+b1` con `/usr/bin/lldb-dap-19` según Debian. |
| Xvfb | `xvfb-run` ausente en PATH. Su uso para CI gráfico se evalúa en Fase 1. |

## Pruebas de auditoría

- **Cargo estable operativo.** En un proyecto binario temporal creado con `cargo new`, verifiqué metadatos offline, formato, Clippy con advertencias como errores, compilación del ejecutor de pruebas y ejecución del binario. La plantilla contenía cero casos de prueba.
- **Artefacto Release localizado.** Ejecuté `cargo build --offline --release --message-format=json`, extraje el campo `executable` del evento `compiler-artifact` y comprobé permiso de ejecución. El proyecto temporal se retiró al terminar.
- **Versiones candidatas comprobadas.** `cargo info` confirmó la publicación de los tres bindings y los manifiestos de libadwaita y sourceview5 apuntan a GTK 0.10. La simulación `apt-get -s install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev` resolvió siete paquetes nuevos y cero eliminaciones. Falta comprobar compilación, enlace y apertura de la GUI con esas dependencias instaladas.
- **CA-00 aprobado.** La auditoría produjo los cinco documentos. Los resultados posteriores se registran abajo.

## Preparación pendiente

Para Fase 4, instalar el componente con `rustup component add rust-analyzer --toolchain stable` y verificar su versión e inicialización LSP. Para Fase 5, instalar `lldb-19` y validar lanzamiento DAP e inspección de variables. Durante esta auditoría las consultas Cargo poblaron su caché y APT se usó en modo de consulta y simulación.

## Documentos

- [Arquitectura](ARQUITECTURA.md)
- [Hoja de ruta](HOJA_DE_RUTA.md)
- [Especificación funcional](ESPECIFICACION_FUNCIONAL.md)
- [Criterios de aceptación](CRITERIOS_ACEPTACION.md)

## Implementación verificada

- Inicialicé Git y el workspace. La ventana GTK4/libadwaita se compiló, enlazó y apareció en la sesión X11 de Debian. La prueba gráfica ejercitó el explorador, el buffer GtkSourceView para Rust, la edición y escritura de un archivo real y los tres resultados de la decisión de cierre. La prueba llama a la lógica del diálogo y queda pendiente verificar manualmente los botones, el recorrido de teclado y los estados vacíos y de error de CA-01.
- El menú Proyecto permite crear una aplicación de consola, una biblioteca o un workspace vacío, y abrir un manifiesto existente. Cargo valida los manifiestos y la consola muestra los packages y sus targets. Las pruebas verifican que la creación preserva un destino ya existente y que la ventana procesa un proyecto creado y vuelve a abrir el manifiesto. La selección en los diálogos queda pendiente de verificación manual.
- La aplicación guarda una sesión local con el proyecto, los archivos y la pestaña activa. Una prueba verifica ida y vuelta del formato. El cierre y reinicio del proceso se verificaron en X11 con un directorio de estado temporal.
- Pasaron la compilación, el formato, Clippy con advertencias como errores, nueve pruebas del workspace y la prueba explícita con display. GTK emitió avisos de la configuración local y del tamaño de pestañas durante esa prueba.

## Ejecutar en Debian 13 x86-64

Con los paquetes de desarrollo en el sistema, ejecutar `cargo run --locked -p rustlab-app`. En este equipo las bibliotecas de ejecución ya estaban disponibles y sudo requería contraseña. Los paquetes de desarrollo se prepararon localmente en una carpeta ignorada por Git.

```bash
bash scripts/prepare-debian.sh
source scripts/debian-env.sh
cargo run --locked -p rustlab-app
```

## Avance de proyectos Cargo

- Proyecto ofrece plantillas GTK4 y Cortex-M4F además de consola, biblioteca y workspace. La plantilla embebida fija el target `thumbv7em-none-eabihf` y el mapa de memoria del STM32F303VCT6. Instalé ese target mediante rustup y verifiqué que ambas plantillas especializadas generan un ejecutable con `cargo test -p rustlab-projects --locked -- --ignored`. Queda pendiente la ejecución de la plantilla sobre hardware.
- Archivo permite crear un archivo en la carpeta visible. La creación rechaza destinos existentes y nombres con rutas. Proyecto permite abrir el Cargo.toml raíz y conserva hasta diez proyectos recientes en la sesión local.
- Una prueba valida la importación de un manifiesto miembro de un workspace con espacios en la ruta, dos binarios y un ejemplo. Pasaron el formato, Clippy, las nueve pruebas ordinarias, la prueba GTK con display y la compilación del IDE.

### Gestión de archivos, copias y perfiles

- Archivo permite mover o renombrar el archivo activo después de guardarlo. La operación usa el movimiento local de GIO, rechaza destinos existentes y requiere que origen y destino estén en el mismo sistema de archivos. El IDE reabre el archivo en su nueva ubicación. La prueba verifica el caso exitoso y la preservación de un destino existente.
- Proyecto permite duplicar la copia en disco del proyecto en una carpeta nueva. Omite .git y el target raíz. Los enlaces simbólicos se informan como tipos sin soporte. Si falla una copia, el mensaje indica la ubicación de la copia parcial.
- El selector Debug/Release guarda la preferencia por proyecto para la futura integración con Cargo. La prueba gráfica reabre la ventana con una sesión de prueba y verifica la restauración del proyecto, dos pestañas, la pestaña activa y Release. La serialización a disco se valida en una prueba separada.
- Pasaron el formato, Clippy con advertencias como errores, nueve pruebas ordinarias, la prueba gráfica y la compilación.

### Módulos Rust y reinicio de sesión

- Archivo incluye «Declarar módulo Rust». A partir de una raíz convencional (`main.rs` o `lib.rs`) o de un `mod.rs`, crea el archivo hijo y agrega una declaración `mod` que se puede deshacer en el editor. El usuario guarda explícitamente el padre. Se validan el identificador, la sintaxis del padre y los conflictos con declaraciones o archivos existentes.
- La prueba gráfica activa la acción de menú, rellena el diálogo, crea el hijo y guarda el padre. La prueba de proyectos ejecuta Cargo check sobre un crate real con el módulo generado.
- Dos arranques del binario con un estado XDG temporal, seguidos de un cierre por Alt+F4, confirmaron la restauración y persistencia de proyecto, dos pestañas, pestaña activa y perfil Release.
- Pasaron rustfmt, Clippy, nueve pruebas ordinarias y la prueba gráfica extendida.

## Integración inicial con Cargo

Verificar, Compilar, Ejecutar y Ejecutar pruebas lanzan Cargo fuera del hilo de GTK. Usan el proyecto abierto y el perfil Debug o Release. La salida de ambos streams aparece en vivo. Al terminar se muestran el estado y la duración. La prueba gráfica activa Compilar y Ejecutar con un proyecto real y confirma la existencia del binario Release y la salida de la aplicación. Pasaron también la compilación, rustfmt, Clippy y nueve pruebas ordinarias.

### Salida en vivo, cancelación y diagnósticos

- El crate `rustlab-cargo` ejecuta Cargo en segundo plano y entrega eventos al hilo de GTK. El panel «Problemas y ejecutables» abre diagnósticos en la ubicación reportada por Cargo y ofrece abrir la carpeta de los ejecutables anunciados por Cargo.
- Detener cancela el grupo de procesos en Linux y el cierre de la ventana espera el resultado de la cancelación. La prueba de procesos comprobó recepción de salida antes de terminar y cancelación de un proceso descendiente.
- Pasaron diez pruebas ordinarias, compilación, rustfmt y Clippy. La prueba gráfica compiló y ejecutó un proyecto Release y abrió un diagnóstico real de tipo en el archivo y columna reportados.

## Temas y barra de herramientas

Ver → Tema de colores ofrece Sistema, Claro, Oscuro, Cobalto y Solarizado claro u oscuro. Las paletas de GtkSourceView se aplican al editor abierto y a las pestañas nuevas. La elección se guarda al cerrar normalmente. La barra superior usa iconos simbólicos con etiquetas accesibles y ayudas emergentes, y destaca Verificar y Ejecutar con botones circulares sobre fondo turquesa. Compilación, Clippy, pruebas ordinarias y prueba gráfica aprobados. Captura revisada en Debian 13 con el tema Cobalto.

## Pendientes de entrega

- Fase 1. Completar la verificación manual de CA-01.
- Fase 2. CA-02 verificado con los selectores nativos para consola, importación, duplicación y renombrado y con pruebas de Cargo para las otras plantillas. CA-12 básico verificado en dos arranques. Queda por ejecutar la plantilla embebida en hardware.
- Fase 3. Faltan la selección de binario y ejemplo, las opciones de target y features, argumentos y entorno, y las operaciones restantes de Cargo. Queda validar desde la interfaz la apertura de la carpeta de un ejecutable y la cancelación durante una tarea real.
- Fase 4. Autocompletado semántico implementado para archivos Rust de un proyecto Cargo abierto. Quedan las demás acciones LSP.
- Fases 5 a 8. Pendientes DAP, herramientas profesionales, aprendizaje, extensiones y distribución.

### Autocompletado Rust

Rust-analyzer sugiere símbolos mientras se escribe, usando el contenido sin guardar. Ctrl+Espacio solicita sugerencias, las flechas recorren la lista, Tab o Enter insertan la selección y Escape cierra la lista. El editor acepta snippets de rust-analyzer mediante GtkSourceView y resuelve ediciones adicionales para autoimportar símbolos. Los candidatos se ordenan según la prioridad del servidor. Verificado con rust-analyzer estable 1.99.1 y un proyecto Cargo real, tanto por LSP como en la prueba gráfica.
