extern crate hidapi;

use bytemuck::{bytes_of, AnyBitPattern, Contiguous, NoUninit, Pod, Zeroable};
use const_crc32::crc32_seed;
use crc32fast::Hasher;
use hidapi::HidApi;

const USB_VID: u16 = 0x054c;
const USB_PID: u16 = 0x0ce6;
const DS_BATTERY_STATUS: usize = 78;
const OUTPUT_REPORT_SIZE: usize = 78;

const PS_INPUT_CRC32_SEED: u32 = 0xA1;
const PS_OUTPUT_CRC32_SEED: u8 = 0xA2;
const PS_FEATURE_CRC32_SEED: u32 = 0xA3;

const DS_INPUT_REPORT_USB: u8 = 0x01;
const DS_INPUT_REPORT_USB_SIZE: u8 = 64;
const DS_INPUT_REPORT_BT: u8 = 0x31;
const DS_INPUT_REPORT_BT_SIZE: u8 = 78;

#[repr(C)]
#[derive(Debug)]
struct DualsenseTouchPoint {
    contact: u8,
    x_lo: u8,
    x_hi: u8,
    y_lo: u8,
}

#[repr(C)]
#[derive(Debug)]
struct DualSenseInputReport {
    x: u8,
    y: u8,
    rx: u8,
    ry: u8,
    z: u8,
    rz: u8,
    seq_number: u8,
    buttons: [u8; 4],
    reserved: [u8; 4],

    /* Motion sensors */
    gyro: [u16; 3],  /* x, y, z */
    accel: [u16; 3], /* x, y, z */
    sensor_timestamp: u32,
    reserved2: u8,

    /* Touchpad */
    // struct dualsense_touch_point points[2];
    //may add support later
    touchpad: [DualsenseTouchPoint; 2],
    //
    reserved3: [u8; 12],
    status: u8,
    reserved4: [u8; 10],
}

#[derive(Debug, Pod, Zeroable, Clone, Copy)]
#[repr(C)]
struct DualSenseReportCommon {
    valid_flag0: u8,
    valid_flag1: u8,

    /* For DualShock 4 compatibility mode. */
    motor_right: u8,
    motor_left: u8,
    /* Audio controls */
    headphone_audio_volume: u8,     /* 0-0x7f */
    speaker_audio_volume: u8,       /* 0-255 */
    internal_microphone_volume: u8, /* 0-0x40 */
    audio_flags: u8,
    mute_button_led: u8,

    power_save_control: u8,

    /* right trigger motor */
    right_trigger_motor_mode: u8,
    right_trigger_param: [u8; 10],

    /* right trigger motor */
    left_trigger_motor_mode: u8,
    left_trigger_param: [u8; 10],

    reserved2: [u8; 4],

    reduce_motor_power: u8,
    audio_flags2: u8, /* 3 first bits: speaker pre-gain */

    /* Led and lightbar */
    valid_flag2: u8,
    reserved3: [u8; 2],
    lightbar_setup: u8,
    led_brightness: u8,
    player_leds: u8,
    lightbar_red: u8,
    lightbar_green: u8,
    lightbar_blue: u8,
}

#[derive(Debug, Pod, Zeroable, Clone, Copy)]
#[repr(C)]
struct DualSenseOutputReportBluetooth {
    report_id: u8, /* 0x31 */
    seq_tag: u8,
    tag: u8,
    common: DualSenseReportCommon,
    reserved: [u8; 24],
    crc32: [u8; 4],
}

#[derive(Debug, Zeroable, Pod, Clone, Copy)]
#[repr(C)]
struct DualSenseOutputReportUSB {
    report_id: u8, /* 0x02 */
    common: DualSenseReportCommon,
    reserved: [u8; 15],
}

fn main() {
    let mut api = HidApi::new().unwrap();

    api.reset_devices().unwrap();
    api.add_devices(USB_VID, USB_PID).unwrap();

    let devices = api.device_list();

    for device_api in devices {
        println!("{:?}", device_api);
        println!("{} {}", device_api.vendor_id(), device_api.product_id());

        let manufacturer_string = device_api.manufacturer_string().unwrap();
        let product_string = device_api.product_string().unwrap();
        let serial_number = device_api.serial_number().unwrap();

        let interface_number = device_api.interface_number();

        println!("{} {}", manufacturer_string, product_string);
        println!("{}", serial_number);
        println!(
            "{}",
            match interface_number {
                -1 => "Bluetooth",
                _ => "USB",
            }
        );

        let device = device_api.open_device(&api).unwrap();
        println!("{:?}", device);

        let mut report_input_data: [u8; DS_BATTERY_STATUS] = [0; DS_BATTERY_STATUS];

        device.read_timeout(&mut report_input_data, 1000).unwrap();

        let dual_sense_input_report: *const DualSenseInputReport =
            report_input_data[1..].as_ptr() as *const DualSenseInputReport;

        let input_report: &DualSenseInputReport = unsafe { &*dual_sense_input_report };

        let battery_percentage = ((input_report.status & 0x0f) as u32) * 100 / 8;

        let mut output_report_data: [u8; OUTPUT_REPORT_SIZE] = [0; OUTPUT_REPORT_SIZE];

        device
            .get_report_descriptor(&mut output_report_data)
            .unwrap();

        let dual_sense_output_report: *mut DualSenseOutputReportBluetooth =
            output_report_data.as_ptr() as *mut DualSenseOutputReportBluetooth;

        let output_report: &mut DualSenseOutputReportBluetooth =
            unsafe { &mut *dual_sense_output_report };

        let seed = PS_OUTPUT_CRC32_SEED;
        let mut crc = crc32_seed(&0xFFFFFFFFu32.to_le_bytes(), seed as u32);

        println!("crc {}", crc);

        let mut hasher = Hasher::new_with_initial(crc);
        hasher.update(&output_report_data);
        crc = !hasher.finalize();

        println!("crc {}", crc);
        output_report.common.valid_flag1 = 1 << 2;
        output_report.common.lightbar_red = 255;
        output_report.common.lightbar_blue = 0;
        output_report.common.lightbar_green = 255;
        output_report.common.led_brightness = 255;

        output_report.crc32 = 1403220181u32.to_le_bytes();
        output_report.seq_tag = 0x0;

        output_report.tag = 0x10;

        device.write(output_report_data.as_mut_slice()).unwrap();

        println!("Battery percentage: {}", battery_percentage);
    }
}
