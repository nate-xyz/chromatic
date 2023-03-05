use gtk::prelude::*;
use gtk::gio;

use std::rc::Rc;

use super::window::Window;
use super::recorder::Recorder;

// use super::i18n::i18n_k;

pub fn settings_manager() -> gio::Settings {
    // // We ship a single schema for both default and development profiles
    // let app_id = APPLICATION_ID.trim_end_matches(".Devel");
    let app_id = "io.github.nate_xyz.Chromatic";
    gio::Settings::new(app_id)
}

pub fn active_window() -> Option<gtk::Window> {
    let app = gio::Application::default()
    .expect("Failed to retrieve application singleton")
    .downcast::<gtk::Application>()
    .unwrap();

    let win = app
    .active_window();

    win
}

pub fn recorder() -> Rc<Recorder>{
    active_window()
        .unwrap()
        .downcast::<Window>()
        .unwrap()
        .recorder()
}