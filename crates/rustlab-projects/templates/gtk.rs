use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("org.example.RustLabApp")
        .build();
    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Mi aplicación Rust")
            .default_width(480)
            .default_height(320)
            .content(&gtk::Label::new(Some("¡Hola desde Rust!")))
            .build();
        window.present();
    });
    app.run()
}
