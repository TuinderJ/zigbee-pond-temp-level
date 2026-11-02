use core::ops::Range;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_time::Timer;
use esp_println::println;
use esp_storage::FlashStorage;
use static_cell::StaticCell;
use zigbee::{
    CurrentPowerMode, CurrentPowerSourceLevel, DeviceConfig, LogicalType, NetworkConfig,
    PowerSource, Stack, StackConfig, TimingConfig,
    nwk::nib::CapabilityInformation,
    types::IeeeAddress,
    zcl::{
        clusters::{
            general::{basic::BasicServer, identify::IdentifyServer},
            measurement::temperature::TemperatureMeasurementServer,
        },
        server::UnsupportedClusterResponder,
    },
    zdo::{
        config::DiscoveryType,
        descriptor::{
            DeviceDescriptorConfig, EndpointDescriptor, NodeDescriptorConfig, PowerDescriptorConfig,
        },
    },
};
use zigbee_mac::esp::EspMlme;

use crate::zigbee::distance::server::AnalogInputServer;

use super::{distance, temperature};

const ZIGBEE_FLASH_RANGE: Range<u32> = 0x3f_0000..0x3f_4000;
const EXTENDED_PAN_ID: u64 = 0;
const CHANNELS: Range<u8> = 11..27;
const SCAN_DURATION: u8 = 5;
const POLL_INTERVAL_MS: u32 = 500;
const CAPABILITY: u8 = 0x80;

static ALL_INPUT_CLUSTERS: [u16; 4] = [
    temperature::INPUT_CLUSTERS[0], // basic
    temperature::INPUT_CLUSTERS[1], // identify
    temperature::INPUT_CLUSTERS[2], // temperature measurement
    distance::INPUT_CLUSTERS[2],    // analog input
];

static ENDPOINTS: [EndpointDescriptor; 2] = [
    temperature::ENDPOINT_DESCRIPTOR,
    distance::ENDPOINT_DESCRIPTOR,
];

static BASIC: BasicServer = BasicServer {
    zcl_version: 8,
    application_version: 1,
    stack_version: 0,
    hw_version: 1,
    manufacturer_name: "Tuinder",
    model_identifier: "PondSensor",
    power_source: 0x03,
};

pub static IDENTIFY: IdentifyServer = IdentifyServer::new();

type ZigbeeFlash = zigbee::storage::FlashStorage<BlockingAsync<FlashStorage>>;

type Handler = (
    BasicServer<'static>,
    &'static IdentifyServer,
    &'static TemperatureMeasurementServer<'static>,
    &'static AnalogInputServer<'static>,
    UnsupportedClusterResponder<'static>,
);

pub type AppStack = Stack<'static, EspMlme<'static>, Handler, ZigbeeFlash>;

static STACK: StaticCell<AppStack> = StaticCell::new();

fn stack_config() -> StackConfig<'static> {
    StackConfig::new(
        NetworkConfig {
            extended_pan_id: IeeeAddress(EXTENDED_PAN_ID),
            channels: CHANNELS,
            scan_duration: SCAN_DURATION,
        },
        DeviceConfig {
            logical_type: LogicalType::EndDevice,
            capability_information: CapabilityInformation(CAPABILITY),
            discovery_type: DiscoveryType::default(),
            tc_link_key_exchange: true,
        },
        TimingConfig {
            poll_interval_ms: POLL_INTERVAL_MS,
            ..TimingConfig::default()
        },
        DeviceDescriptorConfig {
            node: NodeDescriptorConfig {
                frequency_band: 0x08,
                manufacturer_code: 0x1037,
                maximum_buffer_size: 80,
                maximum_incoming_transfer_size: 128,
                server_mask: 0,
                maximum_outgoing_transfer_size: 128,
                descriptor_capability_field: 0,
                ..NodeDescriptorConfig::default()
            },
            power: PowerDescriptorConfig {
                current_power_mode: CurrentPowerMode::Stimulated,
                available_power_sources: &[PowerSource::RechargeableBattery],
                current_power_source: PowerSource::RechargeableBattery,
                current_power_source_level: CurrentPowerSourceLevel::Full,
            },
            endpoints: &ENDPOINTS,
        },
    )
}

#[embassy_executor::task]
pub async fn identify_task() {
    loop {
        Timer::after_secs(1).await;
        if IDENTIFY.is_identifying() {
            println!("Identifying ({}s remaining)", IDENTIFY.tick(1));
        }
    }
}

pub async fn init(mac: EspMlme<'static>) -> &'static AppStack {
    let flash = BlockingAsync::new(FlashStorage::new());
    let storage = zigbee::storage::init_with_flash(flash, ZIGBEE_FLASH_RANGE).await;
    let config = stack_config();

    let handler = (
        BASIC,
        &IDENTIFY,
        &temperature::TEMPERATURE,
        &distance::DISTANCE,
        UnsupportedClusterResponder::new(&ALL_INPUT_CLUSTERS),
    );

    STACK.init(AppStack::new(mac, config, handler, storage))
}
