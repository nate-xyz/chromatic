/* window.rs
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
use gtk::{gio, gio::SettingsBindFlags, glib, glib::clone, glib::Receiver};

use std::{cell::{Cell, RefCell}, rc::Rc};
use std::time::{Duration, Instant};
use log::debug;

use pitch_calc::{Hz, LetterOctave};

use super::i18n::i18n;
use super::recorder::Recorder;
use super::gauge::Gauge;
use super::util;
use super::toasts;



#[derive(Clone, Debug)]
pub enum AudioAction {
    RawAudio(Vec<f32>),
    Pitch(f32),
}

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/nate_xyz/Chromatic/window.ui")]
    pub struct Window {
        #[template_child(id = "toast_overlay")]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        #[template_child(id = "note_label")]
        pub note_label: TemplateChild<gtk::Label>,

        #[template_child(id = "frequency_label")]
        pub frequency_label: TemplateChild<gtk::Label>,

        #[template_child(id = "cents_label")]
        pub cents_label: TemplateChild<gtk::Label>,

        #[template_child(id = "gauge_bin")]
        pub gauge_bin: TemplateChild<adw::Bin>,

        #[template_child(id = "gauge_box")]
        pub gauge_box: TemplateChild<gtk::Box>,

        
        #[template_child(id = "note_box")]
        pub note_box: TemplateChild<gtk::Box>,

        pub gauge: RefCell<Option<Gauge>>,
        pub base_pitch: Cell<f64>,
        pub frequency: Cell<f64>,
        pub cents: Cell<i32>,
        pub recorder: Rc<Recorder>,
        pub receiver: RefCell<Option<Receiver<AudioAction>>>,
        pub settings: gio::Settings,
        pub show_gauge: Cell<bool>,

        pub hang_duration: Cell<u64>,
        pub hang_time: RefCell<Option<std::time::Instant>>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "Window";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self{
            let (sender, r) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

            Self {
                toast_overlay: TemplateChild::default(),
                note_label: TemplateChild::default(),
                frequency_label: TemplateChild::default(),
                cents_label: TemplateChild::default(),
                gauge_bin: TemplateChild::default(),
                gauge_box: TemplateChild::default(),
                note_box: TemplateChild::default(),
                gauge: RefCell::new(None),
                base_pitch: Cell::new(440.0),
                frequency: Cell::new(0.0),
                cents: Cell::new(0),
                recorder: Rc::new(Recorder::new(sender)),
                receiver: RefCell::new(Some(r)),
                settings: util::settings_manager(),
                show_gauge: Cell::new(true),
                hang_duration: Cell::new(3),
                hang_time: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for Window {}
    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,        
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Window {
    pub fn new<P: glib::IsA<gtk::Application>>(application: &P) -> Self {
        let window: Window = glib::Object::new(&[("application", application)]);
        window.setup();
        window
    }

    fn setup(&self) {
        let imp = self.imp();
        let gauge = Gauge::new(300, 500);
        imp.gauge_bin.set_child(Some(&gauge));
        
        self.imp().settings
            .bind("show-gauge", imp.gauge_box.upcast_ref::<glib::Object>(), "visible")
            .flags(SettingsBindFlags::DEFAULT)
            .build();

        let show = imp.settings.boolean("show-gauge");
        if show {
            imp.gauge_box.set_height_request(200);
            imp.note_box.set_vexpand(false);
        } else {
            imp.gauge_box.set_height_request(0);
            imp.note_box.set_vexpand(true);
        }
        imp.show_gauge.set(show);

        imp.settings.connect_changed(
            Some("show-gauge"),
            clone!(@strong self as this => move |_settings, _name| {
                let imp = this.imp();
                let show = imp.settings.boolean("show-gauge");
                if show {
                    imp.gauge_box.set_height_request(200);
                    imp.note_box.set_vexpand(false);
                } else {
                    imp.gauge_box.set_height_request(0);
                    imp.note_box.set_vexpand(true);
                }
                imp.show_gauge.set(show);
            }),
        );

        imp.gauge.replace(Some(gauge));

        self.setup_channel();
        self.bind_signals();
    }

    fn setup_channel(&self) {
        let imp = self.imp();
        let receiver = imp.receiver.borrow_mut().take().unwrap();
        receiver.attach(
            None,
            clone!(@strong self as this => move |action| this.clone().process_action(action)),
        );
    }


    fn process_action(&self, action: AudioAction) -> glib::Continue {
        match action {
            AudioAction::RawAudio(buffer) => {
                debug!("BUFFER {:?}", buffer);
            },
            AudioAction::Pitch(freq) => {
                self.update_frequency(freq);
            },
            // _ => debug!("Received action {:?}", action),
        }
        glib::Continue(true)
    }

    fn bind_signals(&self) {
        debug!("bind signals - window");
        let imp = self.imp();

        imp.recorder.connect_local(
            "frequency",
            false,
            clone!(@weak self as this => @default-return None, move |value| {
                let freq_val = value.get(1); 
                match freq_val {
                    Some(freq_val) => {
                        let freq = freq_val.get::<f32>().ok().unwrap();
                        this.update_frequency(freq);
                    },
                    None => (),
                }

                None
            }),
        );

        match imp.recorder.setup() {
            Ok(_) => (),
            Err(e) => {
                debug!("Error recorder setup: {}", e);
                toasts::add_error_toast(i18n("Unable to initialize audio backend."));
            },
        }
    }

    pub fn update_frequency(&self, frequency: f32) {
        let imp = self.imp();
        if frequency <= 0.0 {
            if imp.hang_time.borrow().is_none() {
                imp.hang_time.replace(Some(Instant::now()));
            } else {
                if imp.hang_time.borrow().as_ref().unwrap().elapsed() > Duration::from_secs(imp.hang_duration.get()) {
                    imp.note_label.set_label("<span size=\"400%\">--</span>");
                    imp.frequency_label.set_label("-- Hz");
                    imp.cents_label.set_label("");
                    imp.hang_time.replace(None);
                }
            }

        } else {
            if !imp.hang_time.borrow().is_none() {
                imp.hang_time.replace(None);
            }

            imp.frequency_label.set_label(&format!("{:.2} Hz", frequency));

            let letter_octave = Hz(frequency).letter_octave();

            let letter = match letter_octave.0 as u64 {
                0 => "C",
                1 => "C♯",
                2 => "D♭",
                3 => "D",
                4 => "D♯",
                5 => "E♭",
                6 => "E",
                7 => "F",
                8 => "F♯",
                9 => "G♭",
                10 => "G",
                11 => "G♯",
                12 => "A♭",
                13 => "A",
                14 => "A♯",
                15 => "B♭",
                16 => "B",
                17_u64..=u64::MAX => "?",
            };

            imp.note_label.set_label(&format!("<span size=\"400%\">{}</span><span baseline_shift=\"subscript\" size=\"150%\">{}</span>", letter, letter_octave.1));

            let closest_freq = LetterOctave(letter_octave.0, letter_octave.1).to_hz();

            let cents = (1200.0 * (frequency / closest_freq.0).log2()) as i32;
                        
            if cents > 0 {
                imp.cents_label.set_label(&format!("+{} cents", cents));
            } else if cents < 0 {
                imp.cents_label.set_label(&format!("{} cents", cents));
            } else {
                imp.cents_label.set_label("");
            }

            imp.cents.set(cents);

            if self.imp().show_gauge.get() {
                imp.gauge.borrow().as_ref().unwrap().set_gauge_position(cents);
            }

            
        }
    }

    pub fn add_toast(&self, toast: &adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    pub fn recorder(&self) -> Rc<Recorder> {
        self.imp().recorder.clone()
    }

}
