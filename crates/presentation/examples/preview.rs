//! Randeaza pastila in toate starile, fara fereastra, in preview.png (pentru verificare vizuala).
//! cargo run -p vf-presentation --example preview -- out.png

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Model, PhysicalSize, VecModel};
use std::rc::Rc;
use vf_presentation::{Pill, PillState};

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

const SCALE: f32 = 2.0;

fn render(window: &MinimalSoftwareWindow, w: u32, h: u32) -> Vec<PremultipliedRgbaColor> {
    let mut buf = vec![PremultipliedRgbaColor::default(); (w * h) as usize];
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.render(&mut buf, w as usize);
    });
    buf
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "preview.png".into());
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: SCALE,
    });

    let pill = Pill::new().unwrap();
    let (w, h) = ((120.0 * SCALE) as u32, (44.0 * SCALE) as u32);
    window.set_size(PhysicalSize::new(w, h));
    pill.show().unwrap();

    let speech = [
        0.25, 0.45, 0.7, 0.4, 0.85, 1.0, 0.6, 0.95, 0.5, 0.75, 0.35, 0.5, 0.2,
    ];
    let quiet = [
        0.0, 0.0, 0.05, 0.0, 0.1, 0.25, 0.4, 0.2, 0.05, 0.0, 0.1, 0.0, 0.0,
    ];
    let processing = [
        0.0, 0.0, 0.0, 0.0, 0.18, 0.3, 0.18, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];
    let states: Vec<(PillState, [f32; 13], bool)> = vec![
        (PillState::Recording, speech, false),
        (PillState::Recording, quiet, false),
        (PillState::Recording, speech, true),
        (PillState::Processing, processing, false),
        (PillState::Error, quiet, false),
    ];

    // Imaginea finala: starile una sub alta, peste un fundal inchis si unul deschis.
    let rows = states.len() as u32;
    let mut img = vec![0u8; (w * 2 * h * rows * 4) as usize];
    for (row, (state, levels, hands_free)) in states.into_iter().enumerate() {
        let model = Rc::new(VecModel::from(levels.to_vec()));
        pill.set_levels(model.clone().into());
        pill.set_state(state);
        pill.set_hands_free(hands_free);
        // Lasam animatiile (opacitate, inaltime) sa se termine.
        std::thread::sleep(std::time::Duration::from_millis(250));
        slint::platform::update_timers_and_animations();
        let buf = render(&window, w, h);
        assert_eq!(model.row_count(), 13);
        for (col, bg) in [[40u8, 44, 52], [236, 236, 240]].into_iter().enumerate() {
            for y in 0..h {
                for x in 0..w {
                    let p = buf[(y * w + x) as usize];
                    let a = p.alpha as u32;
                    let px =
                        |c: u8, b: u8| (c as u32 + (b as u32 * (255 - a)) / 255).min(255) as u8;
                    let gx = col as u32 * w + x;
                    let gy = row as u32 * h + y;
                    let i = ((gy * w * 2 + gx) * 4) as usize;
                    img[i..i + 4].copy_from_slice(&[
                        px(p.red, bg[0]),
                        px(p.green, bg[1]),
                        px(p.blue, bg[2]),
                        255,
                    ]);
                }
            }
        }
    }

    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w * 2, h * rows);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img).unwrap();
    println!("{out}");
}
