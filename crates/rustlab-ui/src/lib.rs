mod completion;

use adw::prelude::*;
use gtk::{gio, glib};
use rustlab_cargo::{Event, Job};
use rustlab_editor::{Editor, FileDocument};
use rustlab_projects::{Project, Template};
use rustlab_settings::{BuildProfile, Session, Theme};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

const THEMES: [(Theme, &str, &str); 6] = [
    (Theme::System, "system", "Sistema"),
    (Theme::Light, "light", "Claro"),
    (Theme::Dark, "dark", "Oscuro"),
    (Theme::Cobalt, "cobalt", "Cobalto"),
    (Theme::SolarizedLight, "solarized-light", "Solarizado claro"),
    (Theme::SolarizedDark, "solarized-dark", "Solarizado oscuro"),
];

struct Workspace {
    window: adw::ApplicationWindow,
    tabs: gtk::Notebook,
    files: gtk::ListBox,
    branches: gtk::Label,
    status: gtk::Label,
    output: gtk::TextBuffer,
    explorer_request: std::cell::Cell<u64>,
    project: RefCell<Option<PathBuf>>,
    analyzer: RefCell<Option<std::sync::mpsc::Sender<rustlab_lsp::Task>>>,
    project_profiles: RefCell<BTreeMap<PathBuf, BuildProfile>>,
    profile: gtk::DropDown,
    theme: std::cell::Cell<Theme>,
    theme_action: gio::SimpleAction,
    cargo_job: RefCell<Option<Job>>,
    close_after_cargo: std::cell::Cell<bool>,
    cargo_results: gtk::ListBox,
    directory: RefCell<Option<PathBuf>>,
    explorer_root: RefCell<Option<PathBuf>>,
    explorer_title: gtk::Label,
    recent_projects: RefCell<Vec<PathBuf>>,
    closing_session: RefCell<Option<Session>>,
    restore_session: bool,
    editors: RefCell<Vec<Rc<Editor>>>,
    close_prompt: std::cell::Cell<bool>,
    close_handler: RefCell<Option<glib::SignalHandlerId>>,
}

impl Workspace {
    fn apply_theme(&self, theme: Theme) {
        self.theme.set(theme);
        let style = adw::StyleManager::default();
        style.set_color_scheme(match theme {
            Theme::System => adw::ColorScheme::Default,
            Theme::Light | Theme::SolarizedLight => adw::ColorScheme::ForceLight,
            Theme::Dark | Theme::Cobalt | Theme::SolarizedDark => adw::ColorScheme::ForceDark,
        });
        if let Some((_, id, _)) = THEMES.iter().find(|(value, _, _)| *value == theme) {
            self.theme_action.set_state(&id.to_variant());
        }
        for editor in self.editors.borrow().iter() {
            editor.set_colors(self.editor_colors());
        }
    }

    fn editor_colors(&self) -> &'static str {
        match self.theme.get() {
            Theme::Cobalt => "cobalt",
            Theme::SolarizedLight => "solarized-light",
            Theme::SolarizedDark => "solarized-dark",
            _ if adw::StyleManager::default().is_dark() => "Adwaita-dark",
            _ => "Adwaita",
        }
    }

    fn attach_completion(&self, editor: &Rc<Editor>, overlay: &gtk::Overlay) {
        let Some(root) = self.project.borrow().clone() else {
            return;
        };
        let path = editor.path();
        if !path.starts_with(&root) || path.extension().is_none_or(|extension| extension != "rs") {
            return;
        }
        let mut analyzer = self.analyzer.borrow_mut();
        let worker = analyzer.get_or_insert_with(|| {
            let uri = gio::File::for_path(&root).uri().to_string();
            rustlab_lsp::worker(root, uri)
        });
        completion::attach(
            editor,
            worker.clone(),
            gio::File::for_path(path).uri().to_string(),
            overlay,
        );
    }

    fn report(&self, message: &str) {
        self.status.set_text(message);
        self.output.set_text(message);
    }

    fn run_cargo(self: &Rc<Self>, command: &'static str) {
        let Some(root) = self.project.borrow().clone() else {
            self.report("Abrí un proyecto Cargo primero");
            return;
        };
        if self.cargo_job.borrow().is_some() {
            return;
        }
        for editor in self.editors.borrow().iter() {
            let path = editor.path();
            if !editor.buffer.is_modified() || !path.starts_with(&root) {
                continue;
            }
            if !matches!(command, "check" | "build" | "run") {
                self.report("Guardá los cambios del proyecto antes de ejecutar Cargo");
                return;
            }
            if let Err(error) = editor.save() {
                self.report(&format!("No se pudo guardar {}: {error}", path.display()));
                return;
            }
        }
        while let Some(row) = self.cargo_results.first_child() {
            self.cargo_results.remove(&row);
        }
        self.report(&format!("Ejecutando cargo {command}…\n"));
        let release = self.profile.selected() == 1;
        self.cargo_job
            .replace(Some(rustlab_cargo::start(root, command, release)));
        let started = std::time::Instant::now();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(35), move || {
            let Some(workspace) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            for _ in 0..64 {
                let event = {
                    let job = workspace.cargo_job.borrow();
                    let Some(job) = job.as_ref() else {
                        return glib::ControlFlow::Break;
                    };
                    job.events.try_recv()
                };
                match event {
                    Ok(Event::Output(text)) => {
                        workspace
                            .output
                            .insert(&mut workspace.output.end_iter(), &text);
                        let excess = workspace.output.char_count() - 200_000;
                        if excess > 0 {
                            workspace.output.delete(
                                &mut workspace.output.start_iter(),
                                &mut workspace.output.iter_at_offset(excess),
                            );
                        }
                    }
                    Ok(Event::Diagnostic(diagnostic)) => {
                        let label = format!(
                            "{}:{}:{} · {}",
                            diagnostic.path.display(),
                            diagnostic.line,
                            diagnostic.column,
                            diagnostic.message
                        );
                        let weak = Rc::downgrade(&workspace);
                        workspace.add_cargo_result(&label, move || {
                            if let Some(workspace) = weak.upgrade() {
                                let path = diagnostic.path.clone();
                                let line = diagnostic.line;
                                let column = diagnostic.column;
                                glib::spawn_future_local(async move {
                                    workspace.open_file_at(path, Some((line, column))).await;
                                });
                            }
                        });
                    }
                    Ok(Event::Executable(path)) => {
                        let label = format!("Abrir carpeta de {}", path.display());
                        let weak = Rc::downgrade(&workspace);
                        workspace.add_cargo_result(&label, move || {
                            if let Some(parent) = path.parent() {
                                let uri = gio::File::for_path(parent).uri();
                                let weak = weak.clone();
                                gio::AppInfo::launch_default_for_uri_async(
                                    &uri,
                                    gio::AppLaunchContext::NONE,
                                    gio::Cancellable::NONE,
                                    move |result| {
                                        if let Err(error) = result
                                            && let Some(workspace) = weak.upgrade()
                                        {
                                            workspace.report(&error.to_string());
                                        }
                                    },
                                );
                            }
                        });
                    }
                    Ok(Event::Finished(result, cancelled)) => {
                        workspace.cargo_job.take();
                        let result = if cancelled {
                            "Cancelado".to_owned()
                        } else {
                            match result {
                                Ok(status) => status.to_string(),
                                Err(error) => error.to_string(),
                            }
                        };
                        let status = format!(
                            "cargo {command} · {result} · {:.2} s",
                            started.elapsed().as_secs_f64()
                        );
                        workspace.status.set_text(&status);
                        workspace
                            .output
                            .insert(&mut workspace.output.end_iter(), &format!("\n{status}\n"));
                        if workspace.close_after_cargo.replace(false) {
                            workspace.close(true);
                        }
                        return glib::ControlFlow::Break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        workspace.cargo_job.take();
                        workspace.report("La tarea de Cargo terminó inesperadamente");
                        if workspace.close_after_cargo.replace(false) {
                            workspace.close(true);
                        }
                        return glib::ControlFlow::Break;
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    fn add_cargo_result(&self, label: &str, action: impl Fn() + 'static) {
        // Keep pathological output from allocating an unbounded number of widgets.
        if self.cargo_results.row_at_index(999).is_some() {
            return;
        }
        let button = gtk::Button::with_label(label);
        button.connect_clicked(move |_| action());
        self.cargo_results.append(&button);
    }

    fn select(self: &Rc<Self>, folder: bool) {
        let dialog = gtk::FileDialog::builder()
            .title(if folder {
                "Abrir carpeta"
            } else {
                "Abrir archivo"
            })
            .build();
        let workspace = self.clone();
        let selected = move |result: Result<gio::File, glib::Error>| match result {
            Ok(file) => {
                if let Some(path) = file.path() {
                    if folder {
                        match path.canonicalize() {
                            Ok(root) => {
                                workspace.project.take();
                                workspace.analyzer.take();
                                workspace.profile.set_sensitive(false);
                                workspace.window.set_title(Some("RustLab IDE"));
                                workspace.set_explorer_root(root, None);
                            }
                            Err(error) => workspace.report(&error.to_string()),
                        }
                    } else {
                        workspace.open(path);
                    }
                } else {
                    workspace.report("Seleccioná una ruta local");
                }
            }
            Err(error) if !error.matches(gtk::DialogError::Dismissed) => {
                workspace.report(&error.to_string());
            }
            Err(_) => {}
        };
        if folder {
            dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, selected);
        } else {
            dialog.open(Some(&self.window), gio::Cancellable::NONE, selected);
        }
    }

    fn select_project(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Abrir proyecto Cargo")
            .build();
        let workspace = self.clone();
        dialog.open(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |result| match result {
                Ok(file) => match file.path() {
                    Some(path) => workspace.load_project(move || Project::open(&path)),
                    None => workspace.report("Elegí un manifiesto Cargo local"),
                },
                Err(error) if !error.matches(gtk::DialogError::Dismissed) => {
                    workspace.report(&error.to_string());
                }
                Err(_) => {}
            },
        );
    }

    fn duplicate_project(self: &Rc<Self>) {
        let Some(source) = self.project.borrow().clone() else {
            self.report("Abrí un proyecto Cargo para duplicarlo");
            return;
        };
        let workspace = self.clone();
        let dialog = gtk::FileDialog::builder()
            .title("Elegí una ubicación y un nombre para la copia")
            .initial_name("copia-del-proyecto")
            .build();
        dialog.save(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |result| match result {
                Ok(file) => {
                    if let Some(destination) = file.path() {
                        workspace.load_project(move || {
                            rustlab_projects::duplicate_project(&source, &destination)
                        });
                    } else {
                        workspace.report("Elegí un destino local");
                    }
                }
                Err(error) if !error.matches(gtk::DialogError::Dismissed) => {
                    workspace.report(&error.to_string())
                }
                Err(_) => {}
            },
        );
    }

    fn move_active_file(self: &Rc<Self>) {
        let Some(editor) = self.active() else {
            self.report("Abrí el archivo que querés mover o renombrar");
            return;
        };
        let source = editor.path();
        let chooser = gtk::FileDialog::builder()
            .title("Mover o renombrar archivo")
            .initial_file(&gio::File::for_path(&source))
            .build();
        let workspace = self.clone();
        chooser.save(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let destination = match result {
                Ok(file) => file,
                Err(error) if error.matches(gtk::DialogError::Dismissed) => return,
                Err(error) => return workspace.report(&error.to_string()),
            };
            let Some(path) = destination.path() else {
                return workspace.report("Elegí un destino local");
            };
            if path == source {
                return;
            }
            let Some(index) = workspace
                .editors
                .borrow()
                .iter()
                .position(|open| Rc::ptr_eq(open, &editor))
            else {
                return;
            };
            if editor.buffer.is_modified() {
                return workspace.report("Guardá los cambios del archivo antes de moverlo");
            }
            if let Err(error) = move_file(&source, &destination) {
                return workspace.report(&error.to_string());
            }
            workspace.remove(index as u32);
            workspace.open(path);
            workspace.refresh_explorer();
        });
    }

    fn show_recent(self: &Rc<Self>) {
        let list = gtk::ListBox::new();
        let scroll = gtk::ScrolledWindow::builder().child(&list).build();
        let dialog = gtk::Window::builder()
            .title("Proyectos recientes")
            .transient_for(&self.window)
            .modal(true)
            .default_width(560)
            .default_height(280)
            .child(&scroll)
            .build();
        if self.recent_projects.borrow().is_empty() {
            list.append(&gtk::Label::new(Some("Todavía no abriste proyectos Cargo")));
        }
        for path in self.recent_projects.borrow().clone() {
            let button = gtk::Button::with_label(&path.display().to_string());
            let popup = dialog.downgrade();
            let workspace = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(dialog) = popup.upgrade() {
                    dialog.close();
                }
                if let Some(workspace) = workspace.upgrade() {
                    let path = path.clone();
                    workspace.load_project(move || Project::open(&path));
                }
            });
            list.append(&button);
        }
        dialog.present();
    }

    fn create_file(self: &Rc<Self>, module: bool) {
        let parent = if module {
            let Some(editor) = self.active() else {
                self.report("Abrí la raíz del crate o un mod.rs para declarar el módulo");
                return;
            };
            Some(editor)
        } else {
            None
        };
        let directory = match &parent {
            Some(editor) => editor.path().parent().map(std::path::Path::to_owned),
            None => self.directory.borrow().clone(),
        };
        let Some(directory) = directory else {
            self.report("Abrí una carpeta para crear un archivo");
            return;
        };
        let entry = gtk::Entry::builder()
            .placeholder_text(if module {
                "nombre_del_modulo"
            } else {
                "nombre.rs"
            })
            .build();
        let create = gtk::Button::with_label(if module {
            "Crear módulo"
        } else {
            "Crear archivo"
        });
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        content.append(&entry);
        content.append(&create);
        let popup = gtk::Window::builder()
            .title(if module {
                "Nuevo módulo Rust"
            } else {
                "Nuevo archivo"
            })
            .transient_for(&self.window)
            .modal(true)
            .child(&content)
            .build();
        let weak_popup = popup.downgrade();
        let workspace = self.clone();
        create.connect_clicked(move |_| {
            let Some(popup) = weak_popup.upgrade() else {
                return;
            };
            let name = entry.text().to_string();
            let directory = directory.clone();
            let workspace = workspace.clone();
            let parent = parent.clone();
            let module_source = parent.as_ref().map(|editor| {
                let (start, end) = editor.buffer.bounds();
                (
                    editor.path(),
                    editor.buffer.text(&start, &end, true).to_string(),
                )
            });
            let module_name = name.clone();
            popup.set_sensitive(false);
            glib::spawn_future_local(async move {
                let folder = directory.clone();
                match gio::spawn_blocking(move || {
                    if let Some((path, source)) = module_source {
                        rustlab_projects::create_module(&path, &source, &name)
                    } else {
                        rustlab_projects::create_file(&folder, &name)
                    }
                })
                .await
                {
                    Ok(Ok(path)) => {
                        if let Some(editor) = parent {
                            let mut end = editor.buffer.end_iter();
                            editor.buffer.begin_user_action();
                            editor
                                .buffer
                                .insert(&mut end, &format!("\nmod {module_name};\n"));
                            editor.buffer.end_user_action();
                        }
                        popup.close();
                        workspace.refresh_explorer();
                        workspace.open(path);
                        if module {
                            workspace.status.set_text("Módulo creado. Guardá el archivo padre para aplicar la declaración.");
                        }
                    }
                    Ok(Err(error)) => {
                        popup.set_sensitive(true);
                        gtk::AlertDialog::builder()
                            .message("No se pudo crear el archivo")
                            .detail(error.to_string())
                            .build()
                            .show(Some(&popup));
                    }
                    Err(_) => {
                        popup.set_sensitive(true);
                        workspace.report("Falló la creación del archivo");
                    }
                }
            });
        });
        popup.set_default_widget(Some(&create));
        popup.present();
    }

    fn create_project(self: &Rc<Self>) {
        let name = gtk::Entry::builder()
            .placeholder_text("nombre_del_proyecto")
            .build();
        let template = gtk::DropDown::from_strings(&[
            "Aplicación de consola",
            "Biblioteca",
            "Workspace",
            "Aplicación GTK4",
            "Cortex-M4F · STM32F303VCT6",
        ]);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();
        let dialog = gtk::Window::builder()
            .title("Nuevo proyecto Cargo")
            .transient_for(&self.window)
            .modal(true)
            .child(&content)
            .build();
        let choose = gtk::Button::with_label("Elegir ubicación…");
        content.append(&name);
        content.append(&template);
        let requirements = gtk::Label::builder().wrap(true).max_width_chars(50).build();
        requirements.set_visible(false);
        let hint = requirements.clone();
        template.connect_selected_notify(move |dropdown| {
            let message = match dropdown.selected() {
                3 => "Requiere los paquetes de desarrollo de GTK4 y libadwaita.",
                4 => "Requiere el target thumbv7em-none-eabihf. El mapa de memoria corresponde al STM32F303VCT6.",
                _ => "",
            };
            hint.set_text(message);
            hint.set_visible(!message.is_empty());
        });
        content.append(&requirements);
        content.append(&choose);
        let workspace = self.clone();
        let popup = dialog.downgrade();
        choose.connect_clicked(move |_| {
            if let Some(popup) = popup.upgrade() {
                popup.close();
            }
            let name = name.text().to_string();
            let template = match template.selected() {
                1 => Template::Library,
                2 => Template::Workspace,
                3 => Template::Gtk,
                4 => Template::CortexM,
                _ => Template::Console,
            };
            let workspace = workspace.clone();
            let chooser = gtk::FileDialog::builder()
                .title("Carpeta para el nuevo proyecto")
                .build();
            if let Some(directory) = workspace.directory.borrow().as_ref() {
                chooser.set_initial_folder(Some(&gio::File::for_path(directory)));
            }
            chooser.select_folder(
                Some(&workspace.window.clone()),
                gio::Cancellable::NONE,
                move |result| match result {
                    Ok(file) => match file.path() {
                        Some(parent) => workspace
                            .load_project(move || Project::create(&parent, &name, template)),
                        None => workspace.report("Elegí una carpeta local"),
                    },
                    Err(error) if !error.matches(gtk::DialogError::Dismissed) => {
                        workspace.report(&error.to_string())
                    }
                    Err(_) => {}
                },
            );
        });
        dialog.present();
    }

    fn load_project(
        self: &Rc<Self>,
        load: impl FnOnce() -> std::io::Result<Project> + Send + 'static,
    ) {
        self.report("Leyendo proyecto Cargo…");
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            match gio::spawn_blocking(load).await {
                Ok(Ok(project)) => {
                    workspace.window.set_title(Some(&format!(
                        "RustLab IDE · {}",
                        project.workspace_root.display()
                    )));
                    let summary = project
                        .packages
                        .iter()
                        .map(|package| {
                            let targets = package
                                .targets
                                .iter()
                                .map(|target| {
                                    format!("{} ({})", target.name, target.kind.join(", "))
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{} · {targets}", package.name)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if workspace.project.borrow().as_ref() != Some(&project.workspace_root) {
                        workspace.analyzer.take();
                    }
                    workspace
                        .project
                        .replace(Some(project.workspace_root.clone()));
                    let profile = workspace
                        .project_profiles
                        .borrow()
                        .get(&project.workspace_root)
                        .copied()
                        .unwrap_or_default();
                    workspace
                        .profile
                        .set_selected(u32::from(profile == BuildProfile::Release));
                    workspace.profile.set_sensitive(true);
                    let mut recent = workspace.recent_projects.borrow_mut();
                    recent.retain(|path| path != &project.workspace_root);
                    recent.insert(0, project.workspace_root.clone());
                    recent.truncate(10);
                    workspace.set_explorer_root(project.workspace_root, Some(summary));
                }
                Ok(Err(error)) => workspace.report(&error.to_string()),
                Err(_) => workspace.report("Falló la lectura del proyecto"),
            }
        });
    }

    fn set_explorer_root(self: &Rc<Self>, root: PathBuf, summary: Option<String>) {
        self.explorer_root.replace(Some(root.clone()));
        self.directory.take();
        while let Some(child) = self.files.first_child() {
            self.files.remove(&child);
        }
        self.branches.set_text("Leyendo ramas…");
        self.explore_project(root, summary);
    }

    fn refresh_explorer(self: &Rc<Self>) {
        let root = self.explorer_root.borrow().clone();
        if let Some(root) = root {
            self.explore_project(root, None);
        }
    }

    fn explore_project(self: &Rc<Self>, root: PathBuf, summary: Option<String>) {
        let request = self.explorer_request.get().wrapping_add(1);
        self.explorer_request.set(request);
        let title = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy();
        self.explorer_title.set_text(&title);
        self.explorer_title
            .set_tooltip_text(Some(&root.to_string_lossy()));
        self.directory.replace(Some(root.clone()));
        self.load_tree_level(root.clone(), root.clone(), self.files.clone(), request);
        if let Some(summary) = summary {
            self.output.set_text(&summary);
        }
        self.refresh_branches(root);
    }

    fn refresh_branches(self: &Rc<Self>, root: PathBuf) {
        let request = self.explorer_request.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let branches = gio::spawn_blocking(move || git_branches(&root)).await;
            if let Some(workspace) = weak.upgrade()
                && workspace.explorer_request.get() == request
            {
                workspace
                    .branches
                    .set_text(&branches.unwrap_or_else(|_| "Falló la lectura de Git".into()));
            }
        });
    }

    fn load_tree_level(
        self: &Rc<Self>,
        root: PathBuf,
        directory: PathBuf,
        list: gtk::ListBox,
        request: u64,
    ) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let canonical = directory.canonicalize()?;
                if canonical != directory || !canonical.starts_with(&root) {
                    return Err(std::io::Error::other(
                        "La carpeta está fuera del espacio de trabajo",
                    ));
                }
                let mut entries = Vec::new();
                for entry in std::fs::read_dir(canonical)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if name == ".git" || name == "target" {
                        continue;
                    }
                    let kind = entry.file_type()?;
                    if kind.is_dir() || kind.is_file() {
                        entries.push((!kind.is_dir(), name, entry.path()));
                    }
                }
                entries.sort();
                Ok::<_, std::io::Error>((root, entries))
            })
            .await;
            let Some(workspace) = weak.upgrade() else {
                return;
            };
            if workspace.explorer_request.get() != request {
                return;
            }
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            match result {
                Ok(Ok((root, entries))) => {
                    if entries.is_empty() {
                        list.append(&gtk::Label::new(Some("Carpeta vacía")));
                    }
                    for (file, name, path) in entries {
                        workspace.add_tree_entry(
                            &list,
                            &root,
                            path,
                            &name.to_string_lossy(),
                            !file,
                        );
                    }
                }
                Ok(Err(error)) => list.append(&gtk::Label::new(Some(&error.to_string()))),
                Err(_) => list.append(&gtk::Label::new(Some("Falló la lectura de la carpeta"))),
            }
        });
    }

    fn add_tree_entry(
        self: &Rc<Self>,
        list: &gtk::ListBox,
        root: &std::path::Path,
        path: PathBuf,
        name: &str,
        directory: bool,
    ) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&gtk::Image::from_icon_name(if directory {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        }));
        let label = gtk::Label::new(Some(name));
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&label);
        let workspace = Rc::downgrade(self);
        if directory {
            let children = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .margin_start(16)
                .build();
            let folder = gtk::Expander::builder()
                .label_widget(&row)
                .child(&children)
                .build();
            let root = root.to_owned();
            let request = self.explorer_request.get();
            folder.connect_expanded_notify(move |folder| {
                if folder.is_expanded()
                    && let Some(workspace) = workspace.upgrade()
                    && workspace.explorer_request.get() == request
                {
                    workspace.directory.replace(Some(path.clone()));
                    workspace.load_tree_level(
                        root.clone(),
                        path.clone(),
                        children.clone(),
                        request,
                    );
                }
            });
            list.append(&folder);
        } else {
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.set_child(Some(&row));
            button.set_tooltip_text(Some(name));
            button.update_property(&[gtk::accessible::Property::Label(name)]);
            let root = root.to_owned();
            button.connect_clicked(move |_| {
                if let Some(workspace) = workspace.upgrade() {
                    match path.canonicalize() {
                        Ok(canonical) if canonical == path && canonical.starts_with(&root) => {
                            workspace
                                .directory
                                .replace(canonical.parent().map(std::path::Path::to_owned));
                            workspace.open(canonical);
                        }
                        _ => {
                            workspace.report("El archivo ya no está dentro del espacio de trabajo")
                        }
                    }
                }
            });
            list.append(&button);
        }
    }

    fn open(self: &Rc<Self>, path: PathBuf) {
        let workspace = self.clone();
        glib::spawn_future_local(async move { workspace.open_file(path).await });
    }

    fn restore(self: &Rc<Self>, session: Session) {
        self.apply_theme(session.theme);
        self.project_profiles.replace(session.project_profiles);
        self.recent_projects.replace(session.recent_projects);
        if let Some(project) = session.project {
            self.project.replace(Some(project.clone()));
            self.load_project(move || Project::open(&project));
        }
        self.restore_files(session.files, session.active);
    }

    fn restore_files(self: &Rc<Self>, files: Vec<PathBuf>, active: Option<usize>) {
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let selected = active.and_then(|index| files.get(index)).cloned();
            for path in files {
                workspace.open_file(path).await;
            }
            if let Some(path) = selected
                && let Some(index) = workspace
                    .editors
                    .borrow()
                    .iter()
                    .position(|editor| editor.matches(&path))
            {
                workspace.tabs.set_current_page(Some(index as u32));
            }
        });
    }

    async fn open_file(self: &Rc<Self>, path: PathBuf) {
        self.open_file_at(path, None).await;
    }

    async fn open_file_at(self: &Rc<Self>, path: PathBuf, location: Option<(u32, u32)>) {
        let (document, text) = match gio::spawn_blocking(move || FileDocument::open(&path)).await {
            Ok(Ok(document)) => document,
            Ok(Err(error)) => return self.report(&error.to_string()),
            Err(_) => return self.report("Falló la apertura del archivo"),
        };
        if let Some(index) = self
            .editors
            .borrow()
            .iter()
            .position(|e| e.matches(document.path()))
        {
            self.tabs.set_current_page(Some(index as u32));
            self.navigate(location);
            return;
        }
        let name = document
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let editor = Rc::new(Editor::new(document, &text));
        editor.set_colors(self.editor_colors());
        let label = gtk::Label::new(Some(&name));
        let close_label = format!("Cerrar {name}");
        let indicator = label.clone();
        editor.buffer.connect_modified_changed(move |buffer| {
            indicator.set_text(&if buffer.is_modified() {
                format!("• {name}")
            } else {
                name.clone()
            });
        });
        let scroll = gtk::ScrolledWindow::builder().child(&editor.view).build();
        let overlay = gtk::Overlay::builder().child(&scroll).build();
        self.attach_completion(&editor, &overlay);
        self.editors.borrow_mut().push(editor.clone());
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.add_css_class("rustlab-tab-close");
        close.set_tooltip_text(Some(&close_label));
        close.update_property(&[gtk::accessible::Property::Label(&close_label)]);
        let workspace = Rc::downgrade(self);
        let page_widget = overlay.downgrade();
        close.connect_clicked(move |_| {
            if let (Some(workspace), Some(page)) = (workspace.upgrade(), page_widget.upgrade()) {
                workspace.close_page(page.upcast_ref(), false);
            }
        });
        let tab = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tab.append(&label);
        tab.append(&close);
        let page = self.tabs.append_page(&overlay, Some(&tab));
        self.tabs.set_show_tabs(true);
        self.tabs.set_current_page(Some(page));
        editor.view.grab_focus();
        self.navigate(location);
    }

    fn navigate(&self, location: Option<(u32, u32)>) {
        if let Some((line, column)) = location
            && let Some(editor) = self.active()
            && let Some(mut iter) = editor.buffer.iter_at_line(line.saturating_sub(1) as i32)
        {
            for _ in 1..column {
                if iter.ends_line() || !iter.forward_char() {
                    break;
                }
            }
            editor.buffer.place_cursor(&iter);
            editor.view.scroll_to_iter(&mut iter, 0.15, false, 0.0, 0.0);
            editor.view.grab_focus();
        }
    }

    fn active(&self) -> Option<Rc<Editor>> {
        self.tabs
            .current_page()
            .and_then(|page| self.editors.borrow().get(page as usize).cloned())
    }

    fn save(&self) -> bool {
        let Some(editor) = self.active() else {
            return true;
        };
        if editor.buffer.is_modified() {
            if let Err(error) = editor.save() {
                self.report(&error.to_string());
                return false;
            }
            self.report("Archivo guardado");
        }
        true
    }

    fn close(self: &Rc<Self>, window: bool) {
        if window && let Some(job) = self.cargo_job.borrow().as_ref() {
            self.close_after_cargo.set(true);
            job.stop();
            self.status.set_text("Cancelando Cargo antes de cerrar…");
            return;
        }
        if self.close_prompt.get() {
            return;
        }
        if window {
            if self.restore_session && self.closing_session.borrow().is_none() {
                self.closing_session.replace(Some(Session {
                    theme: self.theme.get(),
                    project: self.project.borrow().clone(),
                    project_profiles: self.project_profiles.borrow().clone(),
                    recent_projects: self.recent_projects.borrow().clone(),
                    files: self
                        .editors
                        .borrow()
                        .iter()
                        .map(|editor| editor.path())
                        .collect(),
                    active: self.tabs.current_page().map(|page| page as usize),
                }));
            }
            let dirty = self
                .editors
                .borrow()
                .iter()
                .position(|editor| editor.buffer.is_modified());
            if let Some(index) = dirty {
                self.tabs.set_current_page(Some(index as u32));
            } else {
                if let Some(handler) = self.close_handler.borrow_mut().take() {
                    self.window.disconnect(handler);
                }
                if let Some(session) = self.closing_session.borrow_mut().take()
                    && let Err(error) = session.save()
                {
                    eprintln!("No se pudo guardar la sesión: {error}");
                }
                self.window.destroy();
                return;
            }
        }
        if let Some(page) = self.tabs.nth_page(self.tabs.current_page()) {
            self.close_page(&page, window);
        }
    }

    fn close_page(self: &Rc<Self>, page: &gtk::Widget, window: bool) {
        if self.close_prompt.get() {
            return;
        }
        let Some(index) = self.tabs.page_num(page) else {
            return;
        };
        let modified = self.editors.borrow()[index as usize].buffer.is_modified();
        if modified {
            let dialog = gtk::AlertDialog::builder()
                .message("El archivo tiene cambios sin guardar")
                .buttons(["Cancelar", "Descartar", "Guardar"])
                .cancel_button(0)
                .default_button(2)
                .modal(true)
                .build();
            self.close_prompt.set(true);
            let workspace = self.clone();
            let page = page.downgrade();
            dialog.choose(Some(&self.window), gio::Cancellable::NONE, move |choice| {
                workspace.close_prompt.set(false);
                let index = page
                    .upgrade()
                    .and_then(|page| workspace.tabs.page_num(&page));
                if !index.is_some_and(|index| workspace.confirm_close(index, choice.ok())) {
                    workspace.closing_session.take();
                    return;
                }
                if window {
                    workspace.close(true);
                }
            });
        } else {
            self.remove(index);
        }
    }

    fn confirm_close(&self, index: u32, choice: Option<i32>) -> bool {
        match choice {
            Some(1) => {}
            Some(2) => {
                let Some(editor) = self.editors.borrow().get(index as usize).cloned() else {
                    return false;
                };
                if let Err(error) = editor.save() {
                    self.report(&error.to_string());
                    return false;
                }
            }
            _ => return false,
        }
        self.remove(index);
        true
    }

    fn remove(&self, index: u32) {
        let editor = self.editors.borrow_mut().remove(index as usize);
        if let Some(analyzer) = self.analyzer.borrow().as_ref() {
            let _ = analyzer.send(rustlab_lsp::Task::Close(
                gio::File::for_path(editor.path()).uri().to_string(),
            ));
        }
        self.tabs.remove_page(Some(index));
        self.tabs.set_show_tabs(self.tabs.n_pages() > 0);
    }

    fn action(self: &Rc<Self>, name: &str, callback: impl Fn(&Rc<Self>) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let workspace = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(workspace) = workspace.upgrade() {
                callback(&workspace);
            }
        });
        self.window.add_action(&action);
    }
}

fn git_branches(root: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args([
            "for-each-ref",
            "--sort=refname",
            "--format=%(HEAD)%09%(refname)%09%(symref)",
            "refs/heads",
            "refs/remotes",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return format!("No se pudo iniciar Git: {error}"),
    };
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return if error.contains("not a git repository") {
            "Esta carpeta no pertenece a un repositorio Git".into()
        } else {
            error.trim().to_owned()
        };
    }
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(head), Some(reference), Some(symbolic)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !symbolic.is_empty() {
            continue;
        }
        if let Some(name) = reference.strip_prefix("refs/heads/") {
            local.push(if head == "*" {
                format!("{name} (activa)")
            } else {
                name.to_owned()
            });
        } else if let Some(name) = reference.strip_prefix("refs/remotes/") {
            remote.push(name.to_owned());
        }
    }
    let section = |branches: Vec<String>| {
        if branches.is_empty() {
            "Sin ramas".to_owned()
        } else {
            branches.join("\n")
        }
    };
    format!(
        "Locales\n{}\n\nRemotas\n{}",
        section(local),
        section(remote)
    )
}

fn move_file(source: &std::path::Path, destination: &gio::File) -> Result<(), glib::Error> {
    gio::File::for_path(source).move_(
        destination,
        gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE,
        gio::Cancellable::NONE,
        None,
    )
}

pub fn present(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let workspace = present_workspace(app, true);
    match Session::load() {
        Ok(session) => {
            workspace.restore(session);
        }
        Err(error) => workspace.report(&format!("No se pudo restaurar la sesión: {error}")),
    }
}

fn present_workspace(app: &adw::Application, restore_session: bool) -> Rc<Workspace> {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("RustLab IDE")
        .default_width(1150)
        .default_height(760)
        .build();
    let tabs = gtk::Notebook::builder()
        .scrollable(true)
        .show_tabs(false)
        .build();
    let files = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    let status = gtk::Label::builder()
        .label("Abrí un archivo o una carpeta para empezar")
        .xalign(0.0)
        .margin_start(10)
        .margin_bottom(5)
        .margin_top(5)
        .build();
    let output = gtk::TextBuffer::new(None);
    let profile = gtk::DropDown::from_strings(&["Debug", "Release"]);
    profile.set_sensitive(false);
    profile.set_tooltip_text(Some("Perfil del proyecto Cargo"));
    let workspace = Rc::new(Workspace {
        window,
        tabs,
        files,
        branches: gtk::Label::builder()
            .label("Abrí una carpeta para ver sus ramas Git")
            .xalign(0.0)
            .yalign(0.0)
            .selectable(true)
            .wrap(true)
            .margin_start(10)
            .margin_top(10)
            .build(),
        status,
        output,
        explorer_request: std::cell::Cell::new(0),
        project: RefCell::new(None),
        analyzer: RefCell::new(None),
        project_profiles: RefCell::new(BTreeMap::new()),
        profile: profile.clone(),
        theme: std::cell::Cell::new(Theme::System),
        theme_action: gio::SimpleAction::new_stateful(
            "theme",
            Some(glib::VariantTy::STRING),
            &"system".to_variant(),
        ),
        cargo_job: RefCell::new(None),
        close_after_cargo: std::cell::Cell::new(false),
        cargo_results: gtk::ListBox::new(),
        directory: RefCell::new(None),
        explorer_root: RefCell::new(None),
        explorer_title: gtk::Label::builder()
            .label("Abrí un proyecto o una carpeta")
            .xalign(0.0)
            .margin_start(8)
            .margin_top(8)
            .margin_bottom(8)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
        recent_projects: RefCell::new(Vec::new()),
        closing_session: RefCell::new(None),
        restore_session,
        editors: RefCell::new(Vec::new()),
        close_prompt: std::cell::Cell::new(false),
        close_handler: RefCell::new(None),
    });

    let weak_workspace = Rc::downgrade(&workspace);
    profile.connect_selected_notify(move |profile| {
        let Some(workspace) = weak_workspace.upgrade() else {
            return;
        };
        let Some(project) = workspace.project.borrow().clone() else {
            return;
        };
        let selected = if profile.selected() == 1 {
            BuildProfile::Release
        } else {
            BuildProfile::Debug
        };
        workspace
            .project_profiles
            .borrow_mut()
            .insert(project, selected);
    });
    for (action, command) in [
        ("check", "check"),
        ("build", "build"),
        ("run", "run"),
        ("test", "test"),
    ] {
        workspace.action(action, move |window| window.run_cargo(command));
    }
    workspace.action("stop", |w| {
        if let Some(job) = w.cargo_job.borrow().as_ref() {
            job.stop();
            w.status.set_text("Cancelando Cargo…");
        }
    });
    workspace.action("duplicate-project", |w| w.duplicate_project());
    workspace.action("move-file", |w| w.move_active_file());
    workspace.action("new-file", |w| w.create_file(false));
    workspace.action("new-module", |w| w.create_file(true));
    workspace.action("edit-manifest", |w| {
        let project = w.project.borrow().clone();
        if let Some(project) = project {
            w.open(project.join("Cargo.toml"));
        } else {
            w.report("Abrí un proyecto Cargo para editar su manifiesto");
        }
    });
    workspace.action("recent-projects", |w| w.show_recent());
    workspace.action("new-project", |w| w.create_project());
    workspace.action("open-project", |w| w.select_project());
    workspace.action("open-file", |w| w.select(false));
    workspace.action("open-folder", |w| w.select(true));
    workspace.action("save", |w| {
        w.save();
    });
    workspace.action("close-tab", |w| w.close(false));
    workspace.action("undo", |w| {
        if let Some(e) = w.active() {
            e.buffer.undo();
        }
    });
    workspace.action("redo", |w| {
        if let Some(e) = w.active() {
            e.buffer.redo();
        }
    });
    let weak = Rc::downgrade(&workspace);
    workspace
        .theme_action
        .connect_change_state(move |_, state| {
            if let Some(workspace) = weak.upgrade()
                && let Some(id) = state.and_then(glib::Variant::str)
                && let Some((theme, _, _)) = THEMES.iter().find(|(_, key, _)| *key == id)
            {
                workspace.apply_theme(*theme);
            }
        });
    workspace.window.add_action(&workspace.theme_action);
    let style = adw::StyleManager::default();
    let weak = Rc::downgrade(&workspace);
    let handler = style.connect_dark_notify(move |_| {
        if let Some(workspace) = weak.upgrade() {
            for editor in workspace.editors.borrow().iter() {
                editor.set_colors(workspace.editor_colors());
            }
        }
    });
    let handler = RefCell::new(Some(handler));
    workspace.window.connect_destroy(move |_| {
        if let Some(handler) = handler.take() {
            style.disconnect(handler);
        }
    });
    workspace.action("about", |w| {
        gtk::AlertDialog::builder()
            .message("RustLab IDE")
            .detail("Entorno de desarrollo para Rust")
            .build()
            .show(Some(&w.window));
    });
    for (action, shortcut) in [
        ("open-file", "<Primary>o"),
        ("open-folder", "<Primary><Shift>o"),
        ("save", "<Primary>s"),
        ("close-tab", "<Primary>w"),
        ("undo", "<Primary>z"),
        ("redo", "<Primary><Shift>z"),
    ] {
        app.set_accels_for_action(&format!("win.{action}"), &[shortcut]);
    }

    let menu = gio::Menu::new();
    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Archivo",
            &[
                ("Nuevo archivo…", "new-file"),
                ("Declarar módulo Rust…", "new-module"),
                ("Mover o renombrar archivo activo…", "move-file"),
                ("Abrir archivo…", "open-file"),
                ("Abrir carpeta…", "open-folder"),
                ("Guardar", "save"),
                ("Cerrar pestaña", "close-tab"),
            ],
        ),
        ("Editar", &[("Deshacer", "undo"), ("Rehacer", "redo")]),
        ("Ver", &[]),
        (
            "Proyecto",
            &[
                ("Nuevo proyecto…", "new-project"),
                ("Abrir manifiesto Cargo…", "open-project"),
                ("Proyectos recientes…", "recent-projects"),
                ("Duplicar proyecto…", "duplicate-project"),
                ("Editar Cargo.toml", "edit-manifest"),
                ("Abrir carpeta…", "open-folder"),
            ],
        ),
        (
            "Compilar",
            &[
                ("Verificar", "check"),
                ("Compilar", "build"),
                ("Ejecutar pruebas", "test"),
            ],
        ),
        ("Ejecutar", &[("Ejecutar", "run"), ("Detener", "stop")]),
        ("Depurar", &[("Depurar", "debug")]),
        ("Herramientas", &[("Consola", "toggle-console")]),
        ("Ayuda", &[("Acerca de RustLab", "about")]),
    ];
    for (label, actions) in sections {
        let submenu = gio::Menu::new();
        for (label, action) in *actions {
            submenu.append(Some(label), Some(&format!("win.{action}")));
        }
        if *label == "Ver" {
            let themes = gio::Menu::new();
            for (_, id, name) in THEMES {
                let item = gio::MenuItem::new(Some(name), None);
                item.set_action_and_target_value(Some("win.theme"), Some(&id.to_variant()));
                themes.append_item(&item);
            }
            submenu.append_submenu(Some("Tema de colores"), &themes);
        }
        menu.append_submenu(Some(label), &submenu);
    }
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    shell.append(&adw::HeaderBar::new());
    shell.append(&gtk::PopoverMenuBar::from_model(Some(&menu)));
    let toolbar = gtk::Box::builder().spacing(6).margin_start(0).build();
    toolbar.add_css_class("rustlab-toolbar");
    let css = gtk::CssProvider::new();
    css.load_from_data(include_str!("toolbar.css"));
    let display = gtk::prelude::WidgetExt::display(&workspace.window);
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    workspace.window.connect_destroy(move |_| {
        gtk::style_context_remove_provider_for_display(&display, &css);
    });
    for (name, icon, action, primary) in [
        (
            "Verificar",
            "object-select-symbolic",
            Some("win.check"),
            true,
        ),
        (
            "Ejecutar",
            "media-playback-start-symbolic",
            Some("win.run"),
            true,
        ),
        ("Compilar", "system-run-symbolic", Some("win.build"), false),
        ("Depurar", "applications-development-symbolic", None, false),
        (
            "Detener",
            "media-playback-stop-symbolic",
            Some("win.stop"),
            false,
        ),
        (
            "Abrir carpeta",
            "folder-open-symbolic",
            Some("win.open-folder"),
            false,
        ),
        ("Guardar", "document-save-symbolic", Some("win.save"), false),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.add_css_class("rustlab-tool");
        if primary {
            button.add_css_class("rustlab-primary-tool");
        }
        button.update_property(&[gtk::accessible::Property::Label(name)]);
        button.set_tooltip_text(Some(if action.is_some() {
            name
        } else {
            "Depurar · pendiente de implementación"
        }));
        button.set_action_name(action);
        button.set_sensitive(action.is_some());
        toolbar.append(&button);
    }
    toolbar.append(&profile);
    shell.append(&toolbar);

    let file_scroll = gtk::ScrolledWindow::builder()
        .child(&workspace.files)
        .build();
    let explorer = gtk::Notebook::new();
    let files_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    files_panel.append(&workspace.explorer_title);
    file_scroll.set_vexpand(true);
    files_panel.append(&file_scroll);
    explorer.append_page(&files_panel, Some(&gtk::Label::new(Some("Archivos"))));
    let branch_scroll = gtk::ScrolledWindow::builder()
        .child(&workspace.branches)
        .build();
    explorer.append_page(&branch_scroll, Some(&gtk::Label::new(Some("Ramas"))));
    let weak = Rc::downgrade(&workspace);
    explorer.connect_switch_page(move |_, _, page| {
        if page == 1
            && let Some(workspace) = weak.upgrade()
        {
            let root = workspace.explorer_root.borrow().clone();
            if let Some(root) = root {
                workspace.refresh_branches(root);
            }
        }
    });
    let output = gtk::TextView::builder()
        .buffer(&workspace.output)
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let output_scroll = gtk::ScrolledWindow::builder().child(&output).build();
    let console = gtk::Notebook::new();
    console.append_page(&output_scroll, Some(&gtk::Label::new(Some("Salida"))));
    let results = gtk::ScrolledWindow::builder()
        .child(&workspace.cargo_results)
        .build();
    console.append_page(
        &results,
        Some(&gtk::Label::new(Some("Problemas y ejecutables"))),
    );
    let editor_area = gtk::Paned::builder()
        .orientation(gtk::Orientation::Vertical)
        .start_child(&workspace.tabs)
        .end_child(&console)
        .resize_end_child(false)
        .position(475)
        .build();
    let layout = gtk::Paned::builder()
        .start_child(&explorer)
        .end_child(&editor_area)
        .resize_start_child(false)
        .position(240)
        .vexpand(true)
        .build();
    workspace.action("toggle-console", move |_| {
        console.set_visible(!console.is_visible())
    });
    shell.append(&layout);
    shell.append(&workspace.status);
    workspace.window.set_content(Some(&shell));

    let closing = workspace.clone();
    let handler = workspace.window.connect_close_request(move |_| {
        closing.close(true);
        glib::Propagation::Stop
    });
    workspace.close_handler.replace(Some(handler));
    workspace.window.present();
    workspace
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_file_preserves_an_existing_destination() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("fuente.rs");
        let destination = folder.path().join("destino.rs");
        std::fs::write(&source, b"fuente").unwrap();
        std::fs::write(&destination, b"destino").unwrap();
        assert!(move_file(&source, &gio::File::for_path(&destination)).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"destino");
        let next = folder.path().join("nuevo.rs");
        move_file(&source, &gio::File::for_path(&next)).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(next).unwrap(), b"fuente");
    }

    #[test]
    #[ignore = "requiere una sesión gráfica"]
    fn opens_edits_saves_and_closes_a_rust_file() {
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.rustlab.IntegrationTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();

        for args in [
            vec!["init", "-q", "-b", "principal"],
            vec![
                "-c",
                "user.name=RustLab",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "Inicio",
                "--allow-empty",
            ],
            vec!["branch", "desarrollo"],
            vec!["update-ref", "refs/remotes/origin/principal", "HEAD"],
        ] {
            let output = std::process::Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let nested = dir.path().join("src/nested");
        std::fs::create_dir_all(&nested).unwrap();
        let nested_file = nested.join("datos.rs");
        std::fs::write(&nested_file, "").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().parent().unwrap(), dir.path().join("externo"))
            .unwrap();
        let workspace = present_workspace(&app, false);
        workspace.set_explorer_root(dir.path().canonicalize().unwrap(), None);
        let context = glib::MainContext::default();
        let wait_for = |condition: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !condition() {
                assert!(std::time::Instant::now() < deadline);
                context.iteration(false);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        wait_for(&|| workspace.branches.text().contains("origin/principal"));
        assert!(workspace.branches.text().contains("principal (activa)"));
        assert!(workspace.branches.text().contains("desarrollo"));
        wait_for(&|| workspace.files.row_at_index(0).is_some());
        let folder = workspace
            .files
            .row_at_index(0)
            .unwrap()
            .child()
            .unwrap()
            .downcast::<gtk::Expander>()
            .unwrap();
        assert!(!folder.is_expanded());
        assert!(workspace.files.row_at_index(1).is_some());
        assert!(workspace.files.row_at_index(2).is_none());
        folder.set_expanded(true);
        let children = folder.child().unwrap().downcast::<gtk::ListBox>().unwrap();
        wait_for(&|| children.row_at_index(0).is_some());
        let nested = children
            .row_at_index(0)
            .unwrap()
            .child()
            .unwrap()
            .downcast::<gtk::Expander>()
            .unwrap();
        nested.set_expanded(true);
        let nested_children = nested.child().unwrap().downcast::<gtk::ListBox>().unwrap();
        wait_for(&|| nested_children.row_at_index(0).is_some());
        let file = nested_children
            .row_at_index(0)
            .unwrap()
            .child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        file.emit_clicked();
        wait_for(&|| {
            workspace
                .active()
                .is_some_and(|editor| editor.path() == nested_file)
        });
        folder.set_expanded(false);
        assert!(workspace.files.row_at_index(1).is_some());
        workspace.close(false);
        workspace.open(path.clone());
        wait_for(&|| workspace.active().is_some());
        let editor = workspace.active().unwrap();
        gtk::prelude::WidgetExt::activate_action(
            &workspace.window,
            "win.theme",
            Some(&"cobalt".to_variant()),
        )
        .unwrap();
        assert_eq!(workspace.theme.get(), Theme::Cobalt);
        assert_eq!(editor.colors().as_deref(), Some("cobalt"));
        assert!(editor.has_rust_highlighting());
        editor
            .buffer
            .set_text("fn main() { println!(\"RustLab\"); }\n");
        assert!(editor.buffer.is_modified());
        assert!(workspace.save());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "fn main() { println!(\"RustLab\"); }\n"
        );
        editor.buffer.set_text("fn main() { unreachable!(); }");
        assert!(!workspace.confirm_close(0, Some(0)));
        assert!(workspace.active().is_some());
        assert!(workspace.confirm_close(0, Some(1)));
        assert!(workspace.active().is_none());
        workspace.open(dir.path().join("main.rs"));
        wait_for(&|| workspace.active().is_some());
        let editor = workspace.active().unwrap();
        editor.buffer.set_text("fn main() { }");
        assert!(workspace.confirm_close(0, Some(2)));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("main.rs")).unwrap(),
            "fn main() { }"
        );
        workspace.load_project({
            let parent = dir.path().to_owned();
            move || Project::create(&parent, "prueba_ide", Template::Console)
        });
        wait_for(&|| {
            workspace
                .output
                .text(
                    &workspace.output.start_iter(),
                    &workspace.output.end_iter(),
                    true,
                )
                .contains("prueba_ide (bin)")
        });
        workspace.load_project({
            let parent = dir.path().to_owned();
            move || Project::open(&parent.join("prueba_ide/Cargo.toml"))
        });
        wait_for(&|| {
            workspace
                .output
                .text(
                    &workspace.output.start_iter(),
                    &workspace.output.end_iter(),
                    true,
                )
                .contains("prueba_ide (bin)")
        });
        let project = dir.path().join("prueba_ide");
        assert!(workspace.profile.is_sensitive());
        workspace.profile.set_selected(1);
        assert_eq!(
            workspace.project_profiles.borrow().get(&project),
            Some(&BuildProfile::Release)
        );
        workspace.close(true);

        let reopened = present_workspace(&app, false);
        reopened.restore(Session {
            theme: Theme::SolarizedLight,
            project: Some(project.clone()),
            project_profiles: BTreeMap::from([(project.clone(), BuildProfile::Release)]),
            files: vec![project.join("Cargo.toml"), project.join("src/main.rs")],
            active: Some(0),
            ..Session::default()
        });
        wait_for(&|| {
            reopened.profile.is_sensitive()
                && reopened.tabs.n_pages() == 2
                && reopened.tabs.current_page() == Some(0)
        });
        assert_eq!(reopened.profile.selected(), 1);
        assert_eq!(
            reopened.active().unwrap().colors().as_deref(),
            Some("solarized-light")
        );
        assert_eq!(
            reopened.active().unwrap().path(),
            project.join("Cargo.toml")
        );
        reopened.tabs.set_current_page(Some(1));
        gtk::prelude::WidgetExt::activate_action(&reopened.window, "win.new-module", None).unwrap();
        let popup = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|window| window.title().as_deref() == Some("Nuevo módulo Rust"))
            .unwrap();
        let entry = popup
            .child()
            .unwrap()
            .first_child()
            .unwrap()
            .downcast::<gtk::Entry>()
            .unwrap();
        entry.set_text("datos");
        entry
            .next_sibling()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap()
            .emit_clicked();
        wait_for(&|| {
            reopened
                .active()
                .is_some_and(|editor| editor.path() == project.join("src/datos.rs"))
        });
        reopened.tabs.set_current_page(Some(1));
        assert!(reopened.active().unwrap().buffer.is_modified());
        gtk::prelude::WidgetExt::activate_action(&reopened.window, "win.build", None).unwrap();
        wait_for(&|| reopened.cargo_job.borrow().is_none());
        assert!(
            std::fs::read_to_string(project.join("src/main.rs"))
                .unwrap()
                .contains("mod datos;")
        );
        assert!(project.join("target/release/prueba_ide").is_file());
        let main = reopened.active().unwrap();
        let updated = "mod datos;\nfn main() { println!(\"Guardado automático\"); }\n";
        main.buffer.set_text(updated);
        gtk::prelude::WidgetExt::activate_action(&reopened.window, "win.run", None).unwrap();
        wait_for(&|| reopened.cargo_job.borrow().is_none());
        assert!(
            reopened
                .output
                .text(
                    &reopened.output.start_iter(),
                    &reopened.output.end_iter(),
                    true
                )
                .contains("Guardado automático")
        );
        assert_eq!(std::fs::read_to_string(main.path()).unwrap(), updated);
        assert!(!main.buffer.is_modified());
        let path = project.join("src/datos.rs");
        let module = reopened.editors.borrow()[2].clone();
        let broken = "fn error() { let _: u32 = \"texto\"; }\n";
        module.buffer.set_text(broken);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, "// Cambio externo\n").unwrap();
        gtk::prelude::WidgetExt::activate_action(&reopened.window, "win.check", None).unwrap();
        assert!(reopened.cargo_job.borrow().is_none());
        assert!(module.buffer.is_modified());
        assert!(reopened.status.text().contains("No se pudo guardar"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "// Cambio externo\n"
        );
        std::fs::write(&path, original).unwrap();

        gtk::prelude::WidgetExt::activate_action(&reopened.window, "win.check", None).unwrap();
        wait_for(&|| reopened.cargo_job.borrow().is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        assert!(!module.buffer.is_modified());
        let mut selected = None;
        for index in 0.. {
            let Some(row) = reopened.cargo_results.row_at_index(index) else {
                break;
            };
            let button = row.child().unwrap().downcast::<gtk::Button>().unwrap();
            if button
                .label()
                .is_some_and(|text| text.contains("datos.rs:1:"))
            {
                selected = Some(button);
                break;
            }
        }
        selected.unwrap().emit_clicked();
        wait_for(&|| {
            reopened
                .active()
                .is_some_and(|editor| editor.path() == path)
        });
        let editor = reopened.active().unwrap();
        assert!(editor.buffer.cursor_position() > 0);
        let current_page = reopened
            .tabs
            .nth_page(reopened.tabs.current_page())
            .unwrap();
        let other_page = reopened.tabs.nth_page(Some(0)).unwrap();
        let close_button = reopened
            .tabs
            .tab_label(&other_page)
            .unwrap()
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        close_button.emit_clicked();
        assert!(reopened.tabs.page_num(&other_page).is_none());
        assert_eq!(
            reopened.tabs.current_page(),
            reopened.tabs.page_num(&current_page)
        );
        if std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            let editor = reopened.active().unwrap();
            editor.view.grab_focus();
            let code = "struct Punto { coordenada: u32 }\nfn ejemplo(punto: Punto) { punto.coor }";
            editor.buffer.set_text(code);
            let mut cursor = editor.buffer.end_iter();
            cursor.backward_chars(2);
            editor.buffer.place_cursor(&cursor);
            let popup = reopened
                .tabs
                .nth_page(reopened.tabs.current_page())
                .unwrap()
                .last_child()
                .unwrap()
                .downcast::<gtk::Popover>()
                .unwrap();
            wait_for(&|| popup.is_visible());
            let list = popup
                .child()
                .unwrap()
                .downcast::<gtk::ScrolledWindow>()
                .unwrap()
                .child()
                .unwrap()
                .downcast::<gtk::Viewport>()
                .unwrap()
                .child()
                .unwrap()
                .downcast::<gtk::ListBox>()
                .unwrap();
            let row = list.row_at_index(0).unwrap();
            list.emit_by_name::<()>("row-activated", &[&row]);
            assert!(
                editor
                    .buffer
                    .text(&editor.buffer.start_iter(), &editor.buffer.end_iter(), true)
                    .contains("punto.coordenada")
            );
            editor.buffer.set_modified(false);
        }
        reopened.close(true);
    }
}
