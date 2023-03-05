/* application.rs
 *
 * Copyright 2023 nate-xyz
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};


use super::config::VERSION;
use super::Window;

use super::i18n::i18n;

use super::preferences_window::PreferencesWindow;


mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct App {}

    #[glib::object_subclass]
    impl ObjectSubclass for App {
        const NAME: &'static str = "App";
        type Type = super::App;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for App {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.instance();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
        }
    }

    impl ApplicationImpl for App {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.instance();
            // Get the current window or create one if necessary
            let window = if let Some(window) = application.active_window() {
                window
            } else {
                let window = Window::new(&*application);
                window.upcast()
            };

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for App {}
    impl AdwApplicationImpl for App {}
}

glib::wrapper! {
    pub struct App(ObjectSubclass<imp::App>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl App {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::new(&[("application-id", &application_id), ("flags", flags)])
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| app.show_preferences())
            .build();

        self.add_action_entries([quit_action, about_action, preferences_action]).unwrap();
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutWindow::builder()
            .transient_for(&window)
            .application_name("Chromatic")
            .application_icon("io.github.nate_xyz.Chromatic")
            .developer_name("nate-xyz")
            .version(VERSION)
            .developers(vec!["nate-xyz".into()])
            .copyright("© 2023 nate-xyz")
            .license_type(gtk::License::Gpl30Only)
            .website("https://github.com/nate-xyz/chromatic")
            .issue_url("https://github.com/nate-xyz/chromatic/issues")
            .build();

        // Translator credits. Replace "translator-credits" with your name/username, and optionally an email or URL. 
        // One name per line, please do not remove previous names.
        about.set_translator_credits(&i18n("translator-credits"));
        
        // Translators: only replace "Inspired by "
        let ack: String = i18n("Inspired by LINGOT");

        about.add_acknowledgement_section(Some(&ack), 
        &["lingot website https://www.nongnu.org/lingot/", "github repo https://github.com/ibancg/lingot"]);


        about.present();
    }

    fn show_preferences(&self) {
        let preferences = PreferencesWindow::new();
        let window = self.active_window().unwrap();
        preferences.set_transient_for(Some(&window));
        preferences.show();
    }
}
