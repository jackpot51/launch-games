use ectool::{Access, AccessHid, Ec, Error};
use hidapi::HidApi;
use softbuffer::GraphicsContext;
use std::{cmp, io, process, time};
use tiny_skia::{Paint, Pixmap, Rect, Transform};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

struct Entity {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    r: u8,
    g: u8,
    b: u8,
}

impl Entity {
    fn pixel_position(&self) -> (i32, i32) {
        (self.x.floor() as i32, self.y.floor() as i32)
    }

    fn update(&mut self, dt: f64) {
        self.x += self.dx * dt;
        self.y += self.dy * dt;
    }
}

struct Led {
    i: u8,
    color: (u8, u8, u8),
    sync_color: Option<(u8, u8, u8)>,
}

impl Led {
    fn set_color(&mut self, r: u8, g: u8, b: u8) {
        self.color = (r, g, b);
    }

    fn sync(&mut self, ec: &mut Ec<Box<dyn Access>>) {
        if self.sync_color != Some(self.color) {
            println!(
                "Set {} #{:02X}{:02X}{:02X}: {:?}",
                self.i,
                self.color.0,
                self.color.1,
                self.color.2,
                unsafe { ec.led_set_color(self.i, self.color.0, self.color.1, self.color.2) }
            );
            self.sync_color = Some(self.color);
        }
    }
}

fn ec_board(ec: &mut Ec<Box<dyn Access>>) -> Result<String, Error> {
    let data_size = unsafe { ec.access().data_size() };
    let mut data = vec![0; data_size];
    let size = unsafe { ec.board(&mut data)? };
    data.truncate(size);
    String::from_utf8(data).map_err(|err| Error::Io(io::Error::new(io::ErrorKind::Other, err)))
}

fn ec_version(ec: &mut Ec<Box<dyn Access>>) -> Result<String, Error> {
    let data_size = unsafe { ec.access().data_size() };
    let mut data = vec![0; data_size];
    let size = unsafe { ec.version(&mut data)? };
    data.truncate(size);
    String::from_utf8(data).map_err(|err| Error::Io(io::Error::new(io::ErrorKind::Other, err)))
}

fn main() {
    let get_ec = || -> Result<_, Error> {
        let api = HidApi::new()?;
        for info in api.device_list() {
            #[allow(clippy::single_match)]
            match (info.vendor_id(), info.product_id(), info.interface_number()) {
                // System76 launch_1
                (0x3384, 0x0001, 1) |
                // System76 launch_lite_1
                (0x3384, 0x0005, 1) |
                // System76 launch_2
                (0x3384, 0x0006, 1) |
                // System76 launch_heavy_1
                (0x3384, 0x0007, 1) => {
                    let device = info.open_device(&api)?;
                    let access = AccessHid::new(device, 10, 100)?;
                    return Ok(unsafe { Ec::new(access)?.into_dyn() });
                }
                _ => {},
            }
        }
        Err(Error::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "HID device not found",
        )))
    };

    let mut ec = match get_ec() {
        Ok(ec) => ec,
        Err(err) => {
            eprintln!("failed to connect to EC: {:X?}", err);
            process::exit(1);
        }
    };

    println!("EC board: {:?}", ec_board(&mut ec));
    println!("EC version: {:?}", ec_version(&mut ec));

    for layer in 0..4 {
        println!("Set layer {} mode: {:?}", layer, unsafe {
            ec.led_set_mode(layer, 1, 0)
        });
        println!("Set layer {} brightness: {:?}", layer, unsafe {
            ec.led_set_value(0xF0 | layer, 0xFF)
        });
    }

    let ni = 0xFF;
    let indices = vec![
        vec![69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83],
        vec![68, 67, 66, 65, 64, 63, 62, 61, 60, 59, 58, 57, 56, 55, 54],
        vec![39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53],
        vec![38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28, 27, 26, ni, 25],
        vec![12, 11, 10, 09, 08, 07, 06, 05, 04, 03, 02, 01, 00, ni, ni],
        vec![13, 14, 15, 16, 17, ni, 18, 19, 20, 21, ni, 22, 23, 24, ni],
    ];

    let banner = vec![
        b" ###### ####   ",
        b"    #   #      ",
        b"    #   ####   ",
        b" #  #      #   ",
        b" ###   ####    ",
        b"               ",
    ];

    let mut max_col = 0;
    let mut leds = Vec::new();
    for indices_row in indices {
        if indices_row.len() > max_col {
            max_col = indices_row.len();
        }

        let mut leds_row = Vec::new();
        for index in indices_row {
            if index == ni {
                leds_row.push(None);
            } else {
                leds_row.push(Some(Led {
                    i: index,
                    color: (0, 0, 0),
                    sync_color: None,
                }));
            }
        }
        leds.push(leds_row);
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let mut graphics_context = unsafe { GraphicsContext::new(&window, &window) }.unwrap();

    let mut paint = Paint::default();
    let mut pixmap = {
        let size = window.inner_size();
        Pixmap::new(size.width, size.height).unwrap()
    };
    println!("Pixmap size {}, {}", pixmap.width(), pixmap.height());

    let mut player: (usize, usize) = (2, 2);
    let mut entities = Vec::<Entity>::new();
    let mut explosions = Vec::<(i32, i32, f64)>::new();

    let mut last_spawn_time = time::Instant::now();
    let mut last_update_time = time::Instant::now();
    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();

        match event {
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(keycode),
                                ..
                            },
                        ..
                    },
                ..
            } => {
                let mut new_player = player;
                match keycode {
                    VirtualKeyCode::W | VirtualKeyCode::Up => {
                        new_player.1 = new_player.1.saturating_sub(1);
                    }
                    VirtualKeyCode::S | VirtualKeyCode::Down => {
                        new_player.1 = new_player.1.saturating_add(1);
                    }
                    VirtualKeyCode::A | VirtualKeyCode::Left => {
                        new_player.0 = new_player.0.saturating_sub(1);
                    }
                    VirtualKeyCode::D | VirtualKeyCode::Right => {
                        new_player.0 = new_player.0.saturating_add(1);
                    }
                    VirtualKeyCode::Space => {
                        entities.push(Entity {
                            x: player.0 as f64,
                            y: player.1 as f64,
                            dx: 8.0,
                            dy: 0.0,
                            r: 0xFF,
                            g: 0x00,
                            b: 0x00,
                        });
                    }
                    _ => (),
                }
                if let Some(leds_row) = leds.get(new_player.1) {
                    if let Some(led_opt) = leds_row.get(new_player.0) {
                        if led_opt.is_some() {
                            player = new_player;
                        }
                    }
                }
            }
            Event::MainEventsCleared => {
                println!("Update");

                let time = time::Instant::now();
                let dt = (time - last_update_time).as_secs_f64();
                last_update_time = time;

                for explosion in explosions.iter_mut() {
                    explosion.2 += dt;
                }

                explosions.retain(|explosion| explosion.2 < 2.0);

                for entity in entities.iter_mut() {
                    entity.update(dt);
                }

                for i in 0..entities.len() {
                    let a = entities[i].pixel_position();
                    for j in i + 1..entities.len() {
                        let b = entities[j].pixel_position();
                        if a == b {
                            entities[i].x = -1.0;
                            entities[j].x = -1.0;
                            explosions.push((a.0, a.1, 0.0));
                        }
                    }
                }

                entities.retain(|entity| {
                    let pixel = entity.pixel_position();
                    pixel.0 >= 0 && pixel.0 < max_col as i32
                });

                if last_spawn_time.elapsed().as_secs_f64() > 2.0 {
                    println!("Spawn");

                    entities.push(Entity {
                        x: (max_col - 1) as f64,
                        y: ((rand::random::<usize>() % (leds.len() - 2)) + 1) as f64,
                        dx: -2.0,
                        dy: 0.0,
                        r: 0x00,
                        g: 0x00,
                        b: 0xFF,
                    });

                    last_spawn_time = time;
                }

                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                println!("Redraw");

                for (row, leds_row) in leds.iter_mut().enumerate() {
                    for (col, led_opt) in leds_row.iter_mut().enumerate() {
                        if let Some(led) = led_opt {
                            if banner[row][col] == b' ' {
                                led.set_color(0x00, 0x00, 0x00);
                            } else {
                                led.set_color(0xFF, 0xFF, 0xFF);
                            }
                        }
                    }
                }

                for entity in entities.iter() {
                    let pixel = entity.pixel_position();
                    if let Some(leds_row) = leds.get_mut(pixel.1 as usize) {
                        if let Some(led_opt) = leds_row.get_mut(pixel.0 as usize) {
                            if let Some(led) = led_opt {
                                led.set_color(entity.r, entity.g, entity.b);
                            }
                        }
                    }
                }

                for explosion in explosions.iter() {
                    #[rustfmt::skip]
                    let pattern = if explosion.2 < 0.1 {
                        [
                            b" Y ",
                            b"YOY",
                            b" Y ",
                        ]
                    } else if explosion.2 < 0.2 {
                        [
                            b"YOY",
                            b"ORO",
                            b"YOY",
                        ]
                    } else if explosion.2 < 0.3 {
                        [
                            b"ORO",
                            b"R R",
                            b"ORO",
                        ]
                    } else if explosion.2 < 0.4 {
                        [
                            b"R R",
                            b"   ",
                            b"R R",
                        ]
                    } else {
                        [
                            b"   ",
                            b"   ",
                            b"   ",
                        ]
                    };

                    for row in 0..3 {
                        let y = explosion.1 + row - 1;
                        if let Some(leds_row) = leds.get_mut(y as usize) {
                            for col in 0..3 {
                                let x = explosion.0 + col - 1;
                                if let Some(led_opt) = leds_row.get_mut(x as usize) {
                                    if let Some(led) = led_opt {
                                        match pattern[row as usize][col as usize] {
                                            b'R' => led.set_color(0xFF, 0x00, 0x00),
                                            b'O' => led.set_color(0xFF, 0x7F, 0x00),
                                            b'Y' => led.set_color(0xFF, 0xFF, 0x00),
                                            _ => (),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(leds_row) = leds.get_mut(player.1) {
                    if let Some(led_opt) = leds_row.get_mut(player.0) {
                        if let Some(led) = led_opt {
                            led.set_color(0x00, 0xFF, 0x00);
                        }
                    }
                }

                for (row, leds_row) in leds.iter_mut().enumerate() {
                    for (col, led_opt) in leds_row.iter_mut().enumerate() {
                        if let Some(led) = led_opt {
                            led.sync(&mut ec);

                            //HACK: tiny-skia color order does not match softbuffer order
                            paint.set_color_rgba8(led.color.2, led.color.1, led.color.0, 0xFF);
                            pixmap.fill_rect(
                                Rect::from_xywh(
                                    col as f32 * 40.0 + 8.0,
                                    row as f32 * 40.0 + 8.0,
                                    32.0,
                                    32.0,
                                )
                                .unwrap(),
                                &paint,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                }

                graphics_context.set_buffer(
                    bytemuck::cast_slice(pixmap.data()),
                    pixmap.width() as u16,
                    pixmap.height() as u16,
                );
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                println!("Resize {:?}", size);
                if size.width != pixmap.width() || size.height != pixmap.height() {
                    pixmap = Pixmap::new(size.width, size.height).unwrap();
                    println!("Pixmap resize {}, {}", size.width, size.height);
                } else {
                    println!("Pixmap already sized correctly");
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("The close button was pressed; stopping");
                control_flow.set_exit();
            }
            _ => (),
        }
    });
}
