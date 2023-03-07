use gtk::prelude::*;
use gtk::gio;

use std::{cell::RefCell, rc::Rc};

use super::window::Window;
use super::recorder::Recorder;
use super::gauge::Gauge;

pub fn settings_manager() -> gio::Settings {
    // // We ship a single schema for both default and development profiles
    // let app_id = APPLICATION_ID.trim_end_matches(".Devel");
    let app_id = "io.github.nate_xyz.Chromatic";
    gio::Settings::new(app_id)
}


pub fn window() -> Window {
    active_window()
        .unwrap()
        .downcast::<Window>()
        .unwrap()
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


pub fn gauge() -> Rc<RefCell<Option<Gauge>>> {
    active_window()
        .unwrap()
        .downcast::<Window>()
        .unwrap()
        .gauge()
}