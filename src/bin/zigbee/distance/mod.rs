use zigbee::{
    aps::apsde::ApsdeSapConfirmStatus,
    types::ShortAddress,
    zcl::{
        clusters::general::{
            analog_input::{self},
            basic, identify,
        },
        profile,
        reporting::{AttributeReportBuilder, AttributeReporting, ReportingTable},
        sender::{ZclReportTarget, ZclSender},
        types::Uint16,
    },
    zdo::descriptor::EndpointDescriptor,
};

pub mod server;
use server::{AnalogInputServer, PRESENT_VALUE};

pub const ENDPOINT: u8 = 2;
const COORDINATOR_ENDPOINT: u8 = 1;

// Fallback report interval. This should be overridden by the coordinator
// TODO: this is for testing
// pub const DEFAULT_REPORT_INTERVAL_SECS: u32 = 5;
pub const DEFAULT_REPORT_INTERVAL_SECS: u32 = 1_800; // 30 minutes

// Cluster list - advertised to the coordinator during discovery
pub static INPUT_CLUSTERS: [u16; 3] = [
    basic::CLUSTER_ID,
    identify::CLUSTER_ID,
    analog_input::CLUSTER_ID,
];
pub static OUTPUT_CLUSTERS: [u16; 0] = [];

pub static ENDPOINT_DESCRIPTOR: EndpointDescriptor = EndpointDescriptor {
    endpoint: ENDPOINT,
    profile_id: profile::HOME_AUTOMATION,
    device_id: 0x000C, // HA Generic Analog Input
    device_version: 1,
    input_clusters: &INPUT_CLUSTERS,
    output_clusters: &OUTPUT_CLUSTERS,
};

pub static REPORTING: ReportingTable<4> = ReportingTable::new();
pub static DISTANCE: AnalogInputServer = AnalogInputServer::new().with_reporting(&REPORTING);

pub struct DistanceReporter {
    now_secs: u32,
    last_checked_report: u32,
}

impl DistanceReporter {
    pub fn new() -> Self {
        Self {
            now_secs: 0,
            last_checked_report: 0,
        }
    }

    pub fn tick(&mut self) {
        self.now_secs = self.now_secs.saturating_add(1);
    }

    pub fn is_due(&self) -> bool {
        self.now_secs.saturating_sub(self.last_checked_report) >= DEFAULT_REPORT_INTERVAL_SECS
    }

    pub async fn report<D: ZclSender>(&mut self, value: u16, device: &D, seq: u8) -> Option<u16> {
        self.update(value);
        let cluster = zigbee::zcl::types::ClusterId(analog_input::CLUSTER_ID);
        let attribute = PRESENT_VALUE.id();
        self.last_checked_report = self.now_secs;

        let due = if REPORTING.configuration(cluster, attribute).is_some() {
            REPORTING.should_report(cluster, attribute, self.now_secs, value as i64)
        } else {
            true
        };

        if !due {
            return None;
        };

        let mut buf = [0u8; 64];
        let report = AttributeReportBuilder::new(&mut buf, seq)
            .and_then(|frame| frame.push(&PRESENT_VALUE, Uint16(value)))
            .and_then(AttributeReportBuilder::finish)
            .expect("encode distance report");

        let result = device
            .send_attribute_report(
                ZclReportTarget {
                    dst_short: ShortAddress::COORDINATOR.0,
                    src_endpoint: ENDPOINT,
                    dst_endpoint: COORDINATOR_ENDPOINT,
                    profile_id: profile::HOME_AUTOMATION,
                },
                report,
                &mut embassy_time::Delay,
            )
            .await;

        match result {
            Ok(confirm) if confirm.status == ApsdeSapConfirmStatus::Success => {
                REPORTING.reported(cluster, attribute, self.now_secs, value as i64);
                Some(value)
            }
            _ => None,
        }
    }

    fn update(&self, value: u16) {
        DISTANCE.set_present_value(value);
    }
}
