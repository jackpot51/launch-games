use ectool::{Access, AccessHid, Ec, Error};
use hidapi::HidApi;
use std::{env, io, process};

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
    let mut args = env::args().skip(1);
    let h = args.next().unwrap().parse::<u8>().unwrap();
    let s = args.next().unwrap().parse::<u8>().unwrap();
    //TODO: use value?
    //let v = args.next().unwrap().parse::<u8>().unwrap();
    let v = 0xFF;
    println!("{h} {s} {v}");

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
                (0x3384, 0x0007, 1) |
                // System76 launch_3
                (0x3384, 0x0009, 1) |
                // System76 launch_heavy_3
                (0x3384, 0x000A, 1) => {
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
            ec.led_set_mode(layer, 13, 0)
        });
        println!("Set layer {} brightness {}: {:?}", layer, v, unsafe {
            ec.led_set_value(0xF0 | layer, v)
        });
        println!(
            "Set layer {} hue {} saturation {}: {:?}",
            layer,
            h,
            s,
            unsafe { ec.led_set_color(0xF0 | layer, h, s, 0) }
        );
    }
}
