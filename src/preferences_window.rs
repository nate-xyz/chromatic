/* preferences_window.rs
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

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, gio::SettingsBindFlags, glib, glib::clone};

use std::{cell::RefCell, error::Error};
use log::{debug, error};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use pulsectl::controllers::DeviceControl;
use pulsectl::controllers::SourceController;

use super::util;

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/nate_xyz/Chromatic/preferences_window.ui")]
    pub struct PreferencesWindow {
        #[template_child(id = "switch_device_select")]
        pub switch_device_select: TemplateChild<gtk::Switch>,

        #[template_child(id = "switch_gauge_visible")]
        pub switch_gauge_visible: TemplateChild<gtk::Switch>,

        #[template_child(id = "device_row")]
        pub device_row: TemplateChild<adw::ComboRow>,

        #[template_child(id = "buffer_adj")]
        pub buffer_adj: TemplateChild<gtk::Adjustment>,

        #[template_child(id = "gauge_hang_adj")]
        pub gauge_hang_adj: TemplateChild<gtk::Adjustment>,

        #[template_child(id = "label_hang_adj")]
        pub label_hang_adj: TemplateChild<gtk::Adjustment>,

        #[template_child(id = "gauge_rest_adj")]
        pub gauge_rest_adj: TemplateChild<gtk::Adjustment>,

        #[template_child(id = "buffer_spin")]
        pub buffer_spin: TemplateChild<gtk::SpinButton>,

        pub settings: gio::Settings,
        pub devices_model: gtk::StringList,
        pub selected_device: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesWindow {
        const NAME: &'static str = "PreferencesWindow";
        type Type = super::PreferencesWindow;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self {
                switch_device_select: TemplateChild::default(),
                switch_gauge_visible: TemplateChild::default(),
                device_row: TemplateChild::default(),
                buffer_adj: TemplateChild::default(),
                gauge_hang_adj: TemplateChild::default(),
                label_hang_adj: TemplateChild::default(),
                gauge_rest_adj: TemplateChild::default(),
                buffer_spin: TemplateChild::default(),
                settings: util::settings_manager(),
                devices_model: gtk::StringList::new(&[]),
                selected_device: RefCell::new("".to_string()),
            }
        }
    }

    impl ObjectImpl for PreferencesWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            match obj.setup_settings() {
                Ok(_) => (),
                Err(e) => error!("unable to load settings: {}", e),
            }
        }
    }

    impl WidgetImpl for PreferencesWindow {}
    impl WindowImpl for PreferencesWindow {}
    impl AdwWindowImpl for PreferencesWindow {}
    impl PreferencesWindowImpl for PreferencesWindow {}
}

glib::wrapper! {
    pub struct PreferencesWindow(ObjectSubclass<imp::PreferencesWindow>)
    @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow,
    @implements gtk::Accessible;
}

impl PreferencesWindow {
    pub fn new() -> PreferencesWindow {
        let prefences: PreferencesWindow = glib::Object::builder::<PreferencesWindow>().build();
        prefences
    }

    fn setup_settings(&self) -> Result<(), Box<dyn Error>> {
        let imp = self.imp();

        debug!("pref window -> setup");

        let devices = self.input_devices()?;

        debug!("pref window -> devices");

        imp.device_row.set_model(Some(&imp.devices_model));
        for d in devices {
            imp.devices_model.append(&d);
        }

        imp.settings
            .bind("choose-device", &*imp.switch_device_select, "active")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

        imp.settings
            .bind("choose-device", &*imp.device_row, "sensitive")
            .flags(SettingsBindFlags::GET)
            .build();

        imp.settings
            .bind("show-gauge", &*imp.switch_gauge_visible, "active")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

        imp.settings.connect_changed(
            Some("selected-device"),
            clone!(@strong self as this => move |_settings, _name| {
                debug!("Pref window -> settings update selected device");
                let imp = this.imp();
                let device_name = imp.settings.string("selected-device").to_string();


                let selected = imp.device_row.selected();
                let device_name_row = imp.devices_model.string(selected).unwrap().to_string();

                if device_name != device_name_row {
                    match util::recorder().switch_stream(Some(device_name.clone())) {
                        Ok(_) => {
                            debug!("switched streams");
                            imp.device_row.set_subtitle(&device_name);
                            this.set_device_selected(device_name);
                        },
                        Err(e) => debug!("{}", e),
                    }
                } else {
                    debug!("already set from row");
                }


            }),
        );


        imp.settings.connect_changed(
            Some("choose-device"),
            clone!(@strong self as this => move |_settings, _name| {
                debug!("Pref window -> settings choose-device");
                let imp = this.imp();
                let manual = imp.settings.boolean("choose-device");
                let device_name = imp.settings.string("selected-device").to_string();



                if manual {
                    match util::recorder().switch_stream(None) {
                        Ok(_) => debug!("switched streams"),
                        Err(e) => debug!("{}", e),
                    }
                } else {
                    match util::recorder().switch_stream(Some(device_name)) {
                        Ok(_) => debug!("switched streams"),
                        Err(e) => debug!("{}", e),
                    }
                }

            }),
        );

        //SET THE DEVICE ROW CURRENT UI FROM SETTINGS
        let device_name = imp.settings.string("selected-device").to_string();
        imp.device_row.set_subtitle(&device_name);
        self.set_device_selected(device_name);

        imp.device_row.connect_selected_notify(
            clone!(@weak self as this => @default-panic, move |_value| {
                debug!("Pref window -> device row select notify");
                let imp = this.imp();
                let selected = imp.device_row.selected();
                let device_name = imp.devices_model.string(selected).unwrap().to_string();
                imp.device_row.set_subtitle(device_name.as_str()); // set the subtitle of the AdwComboRow

                match util::recorder().switch_stream(Some(device_name)) {
                    Ok(_) => debug!("switched streams"),
                    Err(e) => debug!("{}", e),
                }
            }),
        );


        imp.settings
            .bind("buffer-size", &*imp.buffer_adj, "value")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

            imp.settings.connect_changed(
                Some("buffer-size"),
                clone!(@strong self as this => move |_settings, _name| {
                    let imp = this.imp();
                    let device_name = imp.settings.string("selected-device").to_string();
                    imp.buffer_spin.set_sensitive(false);
                    match util::recorder().switch_stream(Some(device_name.clone())) {
                        Ok(_) => {
                            debug!("switched streams");
                            imp.device_row.set_subtitle(&device_name);
                            this.set_device_selected(device_name);
                        },
                        Err(e) => debug!("{}", e),
                    }
                    imp.buffer_spin.set_sensitive(true);
                }),
            );

        imp.settings
            .bind("gauge-hang", &*imp.gauge_hang_adj, "value")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

            imp.settings.connect_changed(
                Some("gauge-hang"),
                clone!(@strong self as this => move |_settings, _name| {    
                    util::gauge().borrow().as_ref().unwrap().start_drawing_thread();
                }),
            );

        imp.settings
            .bind("label-hang", &*imp.label_hang_adj, "value")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

            imp.settings.connect_changed(
                Some("label-hang"),
                clone!(@strong self as this => move |_settings, _name| {    
                    util::window().update_settings();
                }),
            );

        imp.settings
            .bind("gauge-rest-position", &*imp.gauge_rest_adj, "value")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

            imp.settings.connect_changed(
                Some("gauge-rest-position"),
                clone!(@strong self as this => move |_settings, _name| {    
                    util::gauge().borrow().as_ref().unwrap().start_drawing_thread();
                }),
            );

        
        Ok(())
    }

    fn set_device_selected(&self, selected_name: String) {
        let imp = self.imp();
        let mut ratio = 0;
        let matcher = SkimMatcherV2::default();
        let mut index = 0;

        for i in 0..imp.devices_model.n_items() {
            let current_name = imp.devices_model.string(i).unwrap().to_string();

            match matcher.fuzzy_match(&selected_name, &current_name) {
                Some(val) => {
                    if val > ratio {
                        ratio = val;
                        index = i;
                    }
                }
                None => (),
            }
        }

        debug!("settings window updated device row {:#?}", index);

        imp.device_row.set_selected(index);
    }

    //get input devices from pulseaudio
    pub fn input_devices(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut array = Vec::new();
        let mut handler = SourceController::create().unwrap();
        for device in handler.list_devices()? {
            if device.monitor.is_none() {
                array.push(device.description.unwrap());
            }           
        }

        Ok(array)
    }
}
