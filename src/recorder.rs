/* recorder.rs
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
use gtk::{gio, glib, glib::Sender};

use std::{cell::Cell, cell::RefCell, error::Error, rc::Rc, fmt, thread};
use std::sync::mpsc::*;
use log::{debug, error};

//use crossbeam_channel;
use std::sync::mpsc;

use pulsectl::controllers::DeviceControl;
use pulsectl::controllers::SourceController;

use portaudio;

use aubio::{Pitch, PitchMode};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use super::window::AudioAction;
use super::util;
use super::toasts;
use super::i18n::i18n_k;



#[derive(Debug)]
struct RecorderError(String);

impl fmt::Display for RecorderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is an error: {}", self.0)
    }
}

impl Error for RecorderError {}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    #[derive(Debug)]
    pub struct RecorderPriv {
        pub sample_rate: Cell<i64>, //44800
        pub buffer_size: Cell<u32>,
        pub pa: RefCell<Option<Rc<portaudio::PortAudio>>>,
        pub sender: RefCell<Option<Sender<AudioAction>>>,
        pub settings: gio::Settings,
        
        pub tx: RefCell<Option<mpsc::Sender<()>>>,
        // pub rx: RefCell<Option<crossbeam_channel::Receiver<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecorderPriv {
        const NAME: &'static str = "Recorder";
        type Type = super::Recorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                sample_rate: Cell::new(44100),
                buffer_size: Cell::new(1024),
                pa: RefCell::new(None),
                sender: RefCell::new(None),
                settings: util::settings_manager(),
                tx: RefCell::new(None),
                // rx: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for RecorderPriv {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("frequency")
                    .param_types([<f32>::static_type()])
                    .build()]
            });

            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
    pub struct Recorder(ObjectSubclass<imp::RecorderPriv>);
}

impl Default for Recorder {
    fn default() -> Self {
        glib::Object::builder::<Recorder>().build()
    }
}

impl Recorder {
    pub fn new(sender: Sender<AudioAction>) -> Recorder {
        let recorder: Recorder = Self::default();
        recorder.load_sender(sender);
        recorder
    }

    fn load_sender(&self, sender: Sender<AudioAction>) {
        self.imp().sender.replace(Some(sender));
    }


    pub fn setup(&self) -> Result<(), Box<dyn Error>> {
        let imp = self.imp();
        
        //get settings vars
        let manual: bool = imp.settings.boolean("choose-device");
        let manual_device_name = imp.settings.string("selected-device").to_string();

        // Construct a portaudio instance that will connect to a native audio API
        imp.pa.replace(Some(Rc::new(
            portaudio::PortAudio::new().expect("Unable to init PortAudio"),
        )));

        //if manual set, try to stream with manual
        if manual && manual_device_name != "" {
            match self.running_mic_index(manual_device_name.clone()) {
                // and start the stream with the index
                Ok(index) => {
                    debug!("got manual device... {}", manual_device_name);
                    return self.start_stream(Some(index));
                }
                Err(e) => {
                    error!("unable to retrieve device {}", e);
                    toasts::add_error_toast(i18n_k("Unable to retrieve device ({device_name})", &[("device_name", &manual_device_name)]));
                },
            }
        }

        //try to get running input device from pulse audio
        match self.running_device() {
            //if able to retrieve, get the device's index from portaudio
            Ok(device_name) => {
                debug!("retrieved {}", device_name);

                match self.running_mic_index(device_name.clone()) {
                    // and start the stream with the index
                    Ok(index) => {
                        debug!("got running device...");
                        self.start_stream(Some(index))?;
                        imp.settings.set_string("selected-device", &device_name)?;
                        return Ok(());
                    }
                    Err(e) => {
                        error!("{},unable to retrieve device ", e);
                        toasts::add_error_toast(i18n_k("Unable to retrieve device ({device_name})", &[("device_name", &device_name)]));
                    },
                }
            }
            Err(e) => {
                error!("{}, starting stream w/ default option", e);
            }
        }

        //otherwise, just start the stream with the default portaudio option
        return self.start_stream(None);
    }


    pub fn switch_stream(&self, device_name: Option<String>) -> Result<(), Box<dyn Error>> {
        //debug!("retrieved {}", device_name);
        let imp = self.imp();


        if device_name.is_none() {

            let manual: bool = imp.settings.boolean("choose-device");
            let manual_device_name = imp.settings.string("selected-device").to_string();

            if manual && manual_device_name != "" {
                match self.running_mic_index(manual_device_name.clone()) {
                    // and start the stream with the index
                    Ok(index) => {
                        debug!("switch_stream -> got manual device... {}", manual_device_name);
                        return self.start_stream(Some(index));
                    }
                    Err(e) => {
                        error!("switch_stream -> unable to retrieve device {}", e);
                        toasts::add_error_toast(i18n_k("Unable to retrieve device ({device_name})", &[("device_name", &manual_device_name)]));

                    },
                }
            }

        }
        let device_name = device_name.unwrap();

        match self.running_mic_index(device_name.clone()) {
            // and start the stream with the index
            Ok(index) => {
                debug!("switch_stream -> got running device...");
                self.start_stream(Some(index))?;
                imp.settings.set_string("selected-device", &device_name)?;
                return Ok(());
            }
            Err(e) => {
                error!("switch_stream -> {},unable to retrieve device ", e);
                toasts::add_error_toast(i18n_k("Unable to retrieve device ({device_name})", &[("device_name", &device_name)]));
            },
        }

        //otherwise, just start the stream with the default portaudio option
        return self.start_stream(None);
    }

    fn running_device(&self) -> Result<String, Box<dyn Error>> {
        let mut handler = SourceController::create().unwrap();
        for device in handler.list_devices()? {
            if device.state == pulsectl::controllers::types::DevState::Running {
                debug!("Pulse Audio Source: {}", device.name.unwrap());
                return Ok(device.description.unwrap().to_owned());
            }
        }
        return Err(Box::new(RecorderError("No devices found".into())));
    }

    fn running_mic_index(&self, device_name: String) -> Result<portaudio::DeviceIndex, Box<dyn Error>> {
        let mut ratio = 0;
        let mut mic_index = self.pa().default_input_device()?;
        let matcher = SkimMatcherV2::default();

        for device in self.pa().devices()? {
            let (idx, info) = device?;
            let in_channels = info.max_input_channels;

            if in_channels > 0 {
                
                match matcher.fuzzy_match(&device_name, &info.name) {
                    Some(val) => {
                        if val > ratio {
                            ratio = val;
                            mic_index = idx;
                        }
                    }
                    None => (),
                }
            }
        }

        debug!("{:#?} {}", &mic_index, ratio);

        Ok(mic_index)
    }



    pub fn start_stream(&self, mic_option: Option<portaudio::DeviceIndex>) -> Result<(), Box<dyn Error>> {
        // MANY THANKS TO https://dev.to/maniflames/audio-visualization-with-rust-4nhg
        
        let mic_index = match mic_option {
            Some(index) => index,
            None => self.pa().default_input_device()?,
        };
        
        debug!("recorder -> start stream {:?}", mic_index);

        let imp = self.imp();
        let default_low_input_latency = self.pa().device_info(mic_index)?.default_low_input_latency;
        let default_sample_rate = self.pa().device_info(mic_index)?.default_sample_rate;

        // Set parameters for the stream settings.
        // We pass which mic should be used, how many channels are used,
        // whether all the values of all the channels should be passed in a
        // single audiobuffer and the latency that should be considered
        let input_params =
            portaudio::StreamParameters::<f32>::new(mic_index, 1, true, default_low_input_latency);

        // Settings for an inputstream.
        // Here we pass the stream parameters we set before,
        // the sample rate of the mic and the amount values we want to receive
        let buffer_size = 1024 * 6;

        let input_settings =
            portaudio::InputStreamSettings::new(input_params, default_sample_rate, buffer_size);

        // Creating a channel so we can receive audio values asynchronously
        let (sender, receiver) = channel();
        
        // Additional channel to kill thread when switching devices
        //let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let (tx, rx) = mpsc::channel::<()>();

        if !imp.tx.borrow().as_ref().is_none() {
            debug!("killing previous thread with drop send");
            drop(imp.tx.borrow().as_ref()); //drop should kill previous stream 
        }
        imp.tx.replace(Some(tx.clone()));

        // A callback function that should be as short as possible so we send all the info to a different thread
        let callback =
            move | portaudio::InputStreamCallbackArgs { buffer, .. } | match sender.send(buffer) {
                Ok(_) => portaudio::Continue,
                Err(_) => portaudio::Complete,
            };

        // Creating & starting the input stream with our settings & callback
        let mut stream = self
            .pa()
            .open_non_blocking_stream(input_settings, callback)?;

        stream.start()?;
        
        //Printing values every time we receive new ones while the stream is active
        let glib_sender = imp.sender.borrow().as_ref().unwrap().clone();

        //RECEIVE AUDIO BUFFER AND SEND TO GLIB LOOP
        thread::spawn(move || {
            let mut pitch_detector = Pitch::new(
                PitchMode::Yin,
                buffer_size as usize,
                buffer_size as usize / 2,
                default_sample_rate as u32,
            )
            .unwrap();

            debug!("recorder -> stream thread");

            while stream.is_active().unwrap() {
                while let Ok(buffer) = receiver.try_recv() {
                    let pitch = pitch_detector.do_result(&buffer).unwrap();

                    //aubio bugs out sometimes?
                    if pitch < 95999.98 {
                        match glib_sender.send(AudioAction::Pitch(pitch)) {
                            Ok(_) => (),
                            Err(e) => {
                                error!("SEND ERROR {}", e);
                            }
                        }
                    }


                }
                
                //kill stream thread if channel disconnect
                match rx.try_recv() {
                    Ok(_) | Err(mpsc::TryRecvError::Disconnected) => {
                        error!("disconnected channel, stream should end");
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }

            }

            debug!("stream closing ...");
            stream.close().unwrap();
        });

        Ok(())
    }

    fn pa(&self) -> Rc<portaudio::PortAudio> {
        self.imp().pa.borrow().as_ref().unwrap().clone()
    }
}
