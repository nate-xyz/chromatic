/* gauge.rs
 *
 * Copyright 2023 nate-xyz
 *
 * Thanks to lingot!  https://github.com/ibancg/lingot
 * https://github.com/ibancg/lingot/blob/master/src/lingot-gui-gauge.c
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

use gtk::{cairo, glib, glib::clone, glib::Receiver, glib::Sender};

use std::{cell::Cell, cell::RefCell, error::Error, f64::consts::PI};

use log::{debug, error};

use std::thread;
use std::time::Duration;


use crossbeam_channel;

#[derive(Clone, Debug)]
pub enum GaugeAction {
    UpdateGaugePos(i32),
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Gauge {
        pub width: Cell<u32>,
        pub height: Cell<u32>,
        pub gauge_pos: Cell<f64>,
        //pub gauge_end_pos: Cell<f64>,
        pub gauge_range: Cell<f64>,
        pub drawing_area: gtk::DrawingArea,

        pub sender: RefCell<Option<Sender<GaugeAction>>>,
        pub receiver: RefCell<Option<Receiver<GaugeAction>>>,

        pub tx: RefCell<Option<crossbeam_channel::Sender<i32>>>,
        pub rx: RefCell<Option<crossbeam_channel::Receiver<i32>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Gauge {
        const NAME: &'static str = "Gauge";
        type Type = super::Gauge;
        type ParentType = adw::Bin;

        fn new() -> Self {
            let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

            Self {
                width: Cell::new(0),
                height: Cell::new(0),
                gauge_pos: Cell::new(0.0),
                //gauge_end_pos: Cell::new(0.0),
                gauge_range: Cell::new(0.0),
                drawing_area: gtk::DrawingArea::new(),
                sender: RefCell::new(Some(sender)),
                receiver: RefCell::new(Some(receiver)),
                tx: RefCell::new(None),
                rx: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for Gauge {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for Gauge {}
    impl BinImpl for Gauge {}
}

glib::wrapper! {
    pub struct Gauge(ObjectSubclass<imp::Gauge>)
        @extends gtk::Widget, adw::Bin;
}

impl Gauge {
    pub fn new(width: u32, height: u32) -> Gauge {
        let object: Gauge = glib::Object::builder::<Gauge>().build();
        object.setup_channel();
        object.construct(width, height);
        object
    }

    fn setup_channel(&self) {
        let imp = self.imp();
        let receiver = imp.receiver.borrow_mut().take().unwrap();
        receiver.attach(
            None,
            clone!(@strong self as this => move |action| this.clone().process_action(action)),
        );
    }

    fn process_action(&self, action: GaugeAction) -> glib::Continue {
        match action {
            GaugeAction::UpdateGaugePos(pos) => {
                //debug!("FREQUENCY {:?}", freq);
                self.update_gauge_position(pos);
                self.imp().drawing_area.queue_draw();
            }
            // _ => debug!("Received action {:?}", action),
        }

        glib::Continue(true)
    }

    fn construct(&self, width: u32, height: u32) {
        debug!("GAUGE CONSTRUCT");
        let imp = self.imp();
        imp.width.set(width);
        imp.height.set(height);

        self.set_hexpand(true);
        self.set_vexpand(true);
        self.set_halign(gtk::Align::Fill);
        self.set_valign(gtk::Align::Fill);

        imp.gauge_range.set(100.0);

        //let drawing_area = gtk::DrawingArea::new();
        imp.drawing_area.set_hexpand(true);
        imp.drawing_area.set_vexpand(true);
        imp.drawing_area.set_halign(gtk::Align::Fill);
        imp.drawing_area.set_valign(gtk::Align::Fill);
        imp.drawing_area
            .set_draw_func(clone!(@strong self as this => move |_, context, _, _| {
                match this.draw(context) {
                    Ok(_) => (),
                    Err(e) => {
                        error!("{}", e);
                    },
                }
            }));

        self.set_child(Some(&imp.drawing_area));

        self.start_drawing_thread();
    }

    fn update_gauge_position(&self, pos: i32) {
        let pos = pos as f64;
        //debug!("update_gauge_position {}", pos);
        if self.imp().gauge_pos.get() != pos {
            self.imp().gauge_pos.set(pos);
        }
    }

    pub fn set_gauge_position(&self, cents: i32) {
        match self.imp().tx.borrow().as_ref().unwrap().send(cents) {
            Ok(_) => (),
            Err(e) => error!("{}", e),
        }
    }

    //SHOULD ONLY BE CALLED ONCE
    fn start_drawing_thread(&self) {
        let imp = self.imp();
        let glib_sender = imp.sender.borrow().as_ref().unwrap().clone();

        let mut end_goal = 0;
        let mut current_pos = imp.gauge_pos.get() as i32;
        let refresh_milli = 8;
        let hover_amount = (1000 as u64 / 8) as i32;
        let mut hover_time = hover_amount;
        let rest_position = -45;

        //let (tx, rx) = mpsc::channel::<i32>();
        let (tx, rx) = crossbeam_channel::unbounded::<i32>();

        imp.tx.replace(Some(tx));
        imp.rx.replace(Some(rx.clone()));

        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(refresh_milli));
            //println!("Now we should decrease stats and update day count…");

            match rx.try_recv() {
                Ok(new_goal) => {
                    end_goal = new_goal;
                    hover_time = hover_amount;
                },
                Err(_) => {
                    if current_pos == end_goal {
                        //reached goal
                        if end_goal == rest_position {
                            match rx.recv() { //block until new goal
                                Ok(new_goal) => end_goal = new_goal,
                                Err(_) => (),
                            }
                        } else { //hover on location
                            if hover_time > 0 {
                                hover_time -= 1;
                            } else {
                                hover_time = hover_amount;
                                match rx.try_recv() {
                                    Ok(new_goal) => end_goal = new_goal,
                                    Err(_) => end_goal = rest_position,
                                }
                            }
                        }
                    }
                }
            }

            if current_pos != end_goal {
                if (current_pos - end_goal).abs() < 2 {
                    current_pos = end_goal
                } else if current_pos < end_goal {
                    current_pos += 1;
                } else {
                    current_pos -= 1;
                }
            }

            match glib_sender.send(GaugeAction::UpdateGaugePos(current_pos)) {
                Ok(_) => (),
                Err(e) => {
                    error!("SEND ERROR {}", e);
                }
            }
        });
    }

    fn draw(&self, context: &cairo::Context) -> Result<(), Box<dyn Error>> {
        //let bg_color = "#2c3338";
        //let color = self.hex_to_rgb(bg_color);
        context.set_source_rgb(0.0, 0.0, 0.0);
        context.paint()?;
        self.redraw_bg(context)?;
        self.redraw_gauge(context)?;
        Ok(())
    }

    fn draw_gauge_tic(
        &self,
        context: &cairo::Context,
        gauge_center: (f64, f64),
        radius1: f64,
        radius2: f64,
        angle: f64,
    ) -> Result<(), Box<dyn Error>> {
        context.move_to(
            gauge_center.0 + radius1 * angle.sin(),
            gauge_center.1 - radius1 * angle.cos(),
        );
        context.rel_line_to(
            (radius2 - radius1) * angle.sin(),
            (radius1 - radius2) * angle.cos(),
        );
        context.stroke()?;
        Ok(())
    }

    fn hex_to_rgb(&self, hex_string: &str) -> (f64, f64, f64) {
        let r = u8::from_str_radix(&hex_string[1..3], 16).unwrap() as f64 / 255.0;
        let g = u8::from_str_radix(&hex_string[3..5], 16).unwrap() as f64 / 255.0;
        let b = u8::from_str_radix(&hex_string[5..7], 16).unwrap() as f64 / 255.0;
        (r, g, b)
    }

    fn redraw_bg(&self, context: &cairo::Context) -> Result<(), Box<dyn Error>> {
        let gauge_gauge_center_y = 0.94;
        let gauge_cents_bar_stroke = 0.025;
        let gauge_cents_bar_radius = 0.75;
        let gauge_cents_bar_major_tic_radius = 0.04;
        let gauge_cents_bar_minor_tic_radius = 0.03;
        let gauge_cents_bar_major_tic_stroke = 0.03;
        let gauge_cents_bar_minor_tic_stroke = 0.01;
        let gauge_cents_text_size = 0.09;
        let gauge_frequency_bar_stroke = 0.025;
        let gauge_frequency_bar_radius = 0.78;
        let gauge_frequency_bar_major_tic_radius = 0.04;
        let gauge_ok_bar_stroke = 0.07;
        let gauge_ok_bar_radius = 0.48;

        let overture_angle = 65.0 * PI / 180.0;

        //colors
        
        let gauge_frequency_bar_color = "#364da5";
        let gauge_cents_bar_color = "#FFD2C4";
        let gauge_ok_color = "#ABE8FF";
        let gauge_ko_color = "#7486CC";

        // # gauge_cents_bar_color = "#333355"
        // # gauge_frequency_bar_color = "#555533"
        // # gauge_ok_color = "#99dd99"
        // # gauge_ko_color = "#ddaaaa"

        let width = self.width() as f64;
        let height = self.height() as f64;

        let gauge_center = (width / 2.0, height * gauge_gauge_center_y);

        let cents_bar_radius = height * gauge_cents_bar_radius;
        let cents_bar_stroke = height * gauge_cents_bar_stroke;
        let cents_bar_major_tic_radius =
            cents_bar_radius - height * gauge_cents_bar_major_tic_radius;
        let cents_bar_minor_tic_radius =
            cents_bar_radius - height * gauge_cents_bar_minor_tic_radius;
        let cents_bar_major_tic_stroke = height * gauge_cents_bar_major_tic_stroke;
        let cents_bar_minor_tic_stroke = height * gauge_cents_bar_minor_tic_stroke;
        let cents_text_size = height * gauge_cents_text_size;
        let frequency_bar_radius = height * gauge_frequency_bar_radius;
        let frequency_bar_major_tic_radius =
            frequency_bar_radius + height * gauge_frequency_bar_major_tic_radius;
        let frequency_bar_stroke = height * gauge_frequency_bar_stroke;
        let ok_bar_radius = height * gauge_ok_bar_radius;
        let ok_bar_stroke = height * gauge_ok_bar_stroke;

        context.set_source_rgb(1.0, 1.0, 1.0);
        context.save()?;

        // let rect = gdk::Rectangle::new(0, 0, self.width(), self.height());
        // Gdk.cairo_rectangle(context, rect);
        context.fill_preserve()?;
        context.restore()?;

        context.set_source_rgb(0.0, 0.0, 0.0);
        context.stroke()?;

        // #draw ok/ko bar
        context.set_line_width(ok_bar_stroke);
        context.set_line_cap(cairo::LineCap::Butt);
        //context.set_source_rgba(*hex_to_rgba(gauge_ko_color));
        let color = self.hex_to_rgb(gauge_ko_color);
        context.set_source_rgb(color.0, color.1, color.2);
        context.arc(
            gauge_center.0,
            gauge_center.1,
            ok_bar_radius,
            -0.5 * PI - overture_angle,
            -0.5 * PI + overture_angle,
        );
        context.stroke()?;
        //context.set_source_rgba(*hex_to_rgba(gauge_ok_color));
        let color = self.hex_to_rgb(gauge_ok_color);
        context.set_source_rgb(color.0, color.1, color.2);
        context.arc(
            gauge_center.0,
            gauge_center.1,
            ok_bar_radius,
            -0.5 * PI - 0.1 * overture_angle,
            -0.5 * PI + 0.1 * overture_angle,
        );
        context.stroke()?;

        // #draw cents bar
        context.set_line_width(cents_bar_stroke);
        //context.set_source_rgba(*hex_to_rgba(gauge_cents_bar_color));
        let color = self.hex_to_rgb(gauge_cents_bar_color);
        context.set_source_rgb(color.0, color.1, color.2);

        context.arc(
            gauge_center.0,
            gauge_center.1,
            cents_bar_radius,
            -0.5 * PI - 1.05 * overture_angle,
            -0.5 * PI + 1.05 * overture_angle,
        );
        context.stroke()?;

        // #cent tics
        let gauge_range = 100.0;
        let max_minor_divisions = 20.0;
        let cents_per_minor_division: f64 = gauge_range / max_minor_divisions;
        let base = f64::powf(10.0, cents_per_minor_division.log10().floor());
        let mut normalized_cents_per_division = cents_per_minor_division / base;
        if normalized_cents_per_division >= 6.0 {
            normalized_cents_per_division = 10.0;
        } else if normalized_cents_per_division >= 2.5 {
            normalized_cents_per_division = 5.0;
        } else if normalized_cents_per_division >= 1.2 {
            normalized_cents_per_division = 2.0;
        } else {
            normalized_cents_per_division = 1.0;
        }

        let cents_per_minor_division = normalized_cents_per_division * base;
        let cents_per_major_division = 5.0 * cents_per_minor_division;

        // #minor tics
        context.set_line_width(cents_bar_minor_tic_stroke);
        let max_index = (0.5 * gauge_range / cents_per_minor_division).floor() as i32;
        let angle_step = 2.0 * overture_angle * cents_per_minor_division / gauge_range;

        for i in -max_index..max_index + 1 {
            let angle = i as f64 * angle_step;
            self.draw_gauge_tic(
                context,
                gauge_center,
                cents_bar_minor_tic_radius,
                cents_bar_radius,
                angle,
            )?;
        }

        // #major tics
        let max_index = (0.5 * gauge_range / cents_per_major_division).floor() as i32;
        let angle_step = 2.0 * overture_angle * cents_per_major_division / gauge_range;
        context.set_line_width(cents_bar_major_tic_stroke);

        for i in -max_index..max_index + 1 {
            let angle = i as f64 * angle_step;
            self.draw_gauge_tic(
                context,
                gauge_center,
                cents_bar_major_tic_radius,
                cents_bar_radius,
                angle,
            )?;
        }

        // #cents text
        context.set_line_width(1.0);
        let mut old_angle = 0.0;

        context.save()?;

        context.select_font_face(
            "Cantarell",
            cairo::FontSlant::Normal,
            cairo::FontWeight::Normal,
        );
        context.set_font_size(cents_text_size);
        let te = context.text_extents("cent")?;
        context.move_to(
            gauge_center.0 - te.width() / 2.0 - te.x_bearing(),
            gauge_center.1 - 0.81 * cents_bar_major_tic_radius - te.height() / 2.0 - te.y_bearing(),
        );
        context.show_text("cent")?;

        context.translate(gauge_center.0, gauge_center.1);

        for i in -max_index..max_index + 1 {
            let angle = i as f64 * angle_step;
            context.rotate(angle - old_angle);
            let cents = i * cents_per_major_division as i32;
            if i > 0 {
                let text = &format!("+{}", cents);
                let te = context.text_extents(text)?;
                context.move_to(
                    -te.width() / 2.0 - te.x_bearing(),
                    -0.92 * cents_bar_major_tic_radius - te.height() / 2.0 - te.y_bearing(),
                );
                context.show_text(text)?;
                old_angle = angle;
            } else {
                let text = &format!("{}", cents);
                let te = context.text_extents(text)?;
                context.move_to(
                    -te.width() / 2.0 - te.x_bearing(),
                    -0.92 * cents_bar_major_tic_radius - te.height() / 2.0 - te.y_bearing(),
                );
                context.show_text(text)?;
                old_angle = angle;
            }
        }

        context.restore()?;
        context.stroke()?;

        // #draw frequency bar
        context.set_line_width(frequency_bar_stroke);
        // context.set_source_rgba(*hex_to_rgba(gauge_frequency_bar_color))'
        let color = self.hex_to_rgb(gauge_frequency_bar_color);
        context.set_source_rgb(color.0, color.1, color.2);
        context.arc(
            gauge_center.0,
            gauge_center.1,
            frequency_bar_radius,
            -0.5 * PI - 1.05 * overture_angle,
            -0.5 * PI + 1.05 * overture_angle,
        );
        context.stroke()?;

        // #frequency tics
        self.draw_gauge_tic(
            context,
            gauge_center,
            frequency_bar_major_tic_radius,
            frequency_bar_radius,
            0.0,
        )?;

        Ok(())
    }

    fn redraw_gauge(&self, context: &cairo::Context) -> Result<(), Box<dyn Error>> {
        let gauge_size_x = self.width();
        let gauge_size_y = self.height();

        // #normalized dimensions
        let gauge_gauge_center_y = 0.94;
        let gauge_gauge_length = 0.85;
        let gauge_gauge_length_back = 0.08;
        let gauge_gauge_centerradius = 0.045;
        let gauge_gaugestroke = 0.012;
        let gauge_gauge_shadow_offset_x = 0.015;
        let gauge_gauge_shadow_offset_y = 0.01;

        let overture_angle = 65.0 * PI / 180.0;

        // #colors
        let gauge_gauge_color = "#FA9457";
        let gauge_gauge_shadow_color = "#7F7F7F";
        
        // let gauge_gauge_color = "#aa3333";
        // let gauge_gauge_shadow_color = "#7F7F7F";

        let width = gauge_size_x as f64;
        let height = gauge_size_y as f64;

        // #dimensions applied to the current size
        let gauge_center = (width / 2.0, height * gauge_gauge_center_y);

        let gauge_shadow_center = (
            gauge_center.0 + height * gauge_gauge_shadow_offset_x,
            gauge_center.1 + height * gauge_gauge_shadow_offset_y,
        );
        let gauge_length = height * gauge_gauge_length;
        let gauge_length_back = height * gauge_gauge_length_back;
        let gauge_centerradius = height * gauge_gauge_centerradius;
        let gaugestroke = height * gauge_gaugestroke;

        let normalized_error = self.imp().gauge_pos.get() / self.imp().gauge_range.get();
        let angle = 2.0 * normalized_error * overture_angle;
        context.set_line_width(gaugestroke);
        context.set_line_cap(cairo::LineCap::Butt);

        //SHADOW GAUGE
        let color = self.hex_to_rgb(gauge_gauge_shadow_color);
        context.set_source_rgba(color.0, color.1, color.2, 0.25);

        self.draw_gauge_tic(
            context,
            gauge_shadow_center,
            -gauge_length_back,
            -0.99 * gauge_centerradius,
            angle,
        )?;
        self.draw_gauge_tic(
            context,
            gauge_shadow_center,
            0.99 * gauge_centerradius,
            gauge_length,
            angle,
        )?;
        context.arc(
            gauge_shadow_center.0,
            gauge_shadow_center.1,
            gauge_centerradius,
            0.0,
            2.0 * PI,
        );
        context.fill()?;

        //MAIN GAUGE
        let color = self.hex_to_rgb(gauge_gauge_color);
        context.set_source_rgb(color.0, color.1, color.2);

        self.draw_gauge_tic(
            context,
            gauge_center,
            -gauge_length_back,
            gauge_length,
            angle,
        )?;
        context.arc(
            gauge_center.0,
            gauge_center.1,
            gauge_centerradius,
            0.0,
            2.0 * PI,
        );
        context.fill()?;

        Ok(())
    }
}
