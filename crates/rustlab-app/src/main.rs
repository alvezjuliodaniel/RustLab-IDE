use adw::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("org.rustlab.IDE")
        .build();
    app.connect_activate(rustlab_ui::present);
    app.run()
}
