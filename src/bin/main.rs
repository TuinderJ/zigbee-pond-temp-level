#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::{GPIO2, GPIO3, GPIO4};
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use zigbee_mac::esp::EspMlme;

use crate::sensors::distance::DistanceSensor;
use crate::sensors::temperature::TemperatureSensor;
use crate::zigbee::device::{AppStack, identify_task, init};
use crate::zigbee::distance::DistanceReporter;
use crate::zigbee::temperature::TemperatureReporter;

mod sensors;
mod zigbee;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    println!(
        "----------------------------------\nPanic...\n{}\n----------------------------------",
        panic_info.message()
    );
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn stack_task(stack: &'static AppStack) {
    let outcome = stack.run(embassy_time::Delay).await;
    println!("Off the network ({outcome:?}), forgetting it and rebooting");
    stack.forget_network().await;
    esp_hal::system::software_reset();
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.4.0
    // generator parameters: -o esp32c6 -o unstable-hal -o alloc -o esp32c6-mini-1 -o embassy -o neovim-classic -o stable-x86_64-unknown-linux-gnu

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO4
    // - GPIO5
    // - GPIO8
    // - GPIO9
    // - GPIO15

    // Pins actually using.
    // Temperature Sensor 1 wire
    let gpio2 = peripherals.GPIO2;

    // Distance sensor Trigger
    let gpio3 = peripherals.GPIO3;
    // Distance Sensor Echo
    let gpio4 = peripherals.GPIO4;

    // These GPIO pins are in use by some feature of the module and should not be used.
    let _gpio24 = peripherals.GPIO24;
    let _gpio25 = peripherals.GPIO25;
    let _gpio26 = peripherals.GPIO26;
    let _gpio27 = peripherals.GPIO27;
    let _gpio28 = peripherals.GPIO28;
    let _gpio29 = peripherals.GPIO29;
    let _gpio30 = peripherals.GPIO30;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let ieee802154 = Ieee802154::new(peripherals.IEEE802154);
    let mac = EspMlme::new(ieee802154, esp_radio::ieee802154::Config::default());
    println!("Device IEEE address: {:#018x}", mac.ieee_address());
    let stack = init(mac).await;

    spawner.spawn(stack_task(stack).expect("Only one stack task can be running......."));
    spawner.spawn(identify_task().expect("Only one identify task can be running......."));
    println!("Joining network...");
    stack.wait_until_joined().await;
    println!("Network Joined...");
    println!("Network ID: {:?}", stack.config().network().extended_pan_id);
    println!("Channel: {}", stack.config().channel());
    spawner
        .spawn(temperature_task(gpio2, stack).expect("Only one temp task can be running......."));
    spawner.spawn(
        distance_task(gpio3, gpio4, stack).expect("Only one distance task can be running......."),
    );

    loop {
        Timer::after_secs(1).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples
}

#[embassy_executor::task]
async fn temperature_task(gpio2: GPIO2<'static>, stack: &'static AppStack) -> ! {
    let mut delay = esp_hal::delay::Delay::new();
    let mut temp_sensor = TemperatureSensor::new(gpio2);
    let mut temp_reporter = TemperatureReporter::new();
    let mut seq: u8 = 0;

    if let Some(current_temp) = temp_sensor.read(&mut delay).await {
        println!("Current Temp: {:.2} C", current_temp as f64 / 100.0);
        temp_reporter
            .report_if_temperature_changed(current_temp, stack.device(), seq)
            .await;
        seq += 1;
    } else {
        println!("Couldn't read temperature...")
    }

    loop {
        Timer::after_secs(1).await;
        temp_reporter.tick();
        if temp_reporter.is_due() {
            if let Some(current_temp) = temp_sensor.read(&mut delay).await {
                println!("Current Temp: {:.2} C", current_temp as f64 / 100.0);
                temp_reporter
                    .report_if_temperature_changed(current_temp, stack.device(), seq)
                    .await;
                seq += 1;
            } else {
                println!("Couldn't read temperature...")
            }
        }
    }
}

#[embassy_executor::task]
async fn distance_task(
    gpio3: GPIO3<'static>,
    gpio4: GPIO4<'static>,
    stack: &'static AppStack,
) -> ! {
    let mut delay = esp_hal::delay::Delay::new();
    let mut distance_sensor = DistanceSensor::new(gpio3, gpio4);
    let mut distance_reporter = DistanceReporter::new();
    let mut seq: u8 = 0;

    Timer::after_secs(5).await;
    if let Some(current_distance) = distance_sensor.read(&mut delay).await {
        println!("Current Distance: {}", current_distance);
        distance_reporter
            .report(current_distance, stack.device(), seq)
            .await;
        seq += 1;
    } else {
        println!("Couldn't read distance...")
    }

    loop {
        Timer::after_secs(1).await;
        distance_reporter.tick();
        if distance_reporter.is_due() {
            if let Some(current_distance) = distance_sensor.read(&mut delay).await {
                println!("Current Distance: {}", current_distance);
                if let Some(_) = distance_reporter
                    .report(current_distance, stack.device(), seq)
                    .await
                {
                    seq += 1;
                } else {
                    println!("Report failed to send...")
                }
            } else {
                println!("Couldn't read distance...")
            }
        }
    }
}
