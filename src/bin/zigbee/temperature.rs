use zigbee::{
    aps::apsde::ApsdeSapConfirmStatus,
    types::ShortAddress,
    zcl::{
        clusters::{
            general::{basic, identify},
            measurement::temperature::{self, TemperatureMeasurementServer},
        },
        profile,
        reporting::{AttributeReportBuilder, AttributeReporting, ReportingTable},
        sender::{ZclReportTarget, ZclSender},
        types::Int16,
    },
    zdo::descriptor::EndpointDescriptor,
};

pub const ENDPOINT: u8 = 1;
const COORDINATOR_ENDPOINT: u8 = 1;

const MIN_TEMPERATURE: i16 = -4_000;
const MAX_TEMPERATURE: i16 = 12_500;

// Fallback report interval. This should be overridden by the coordinator
pub const DEFAULT_REPORT_INTERVAL_SECS: u32 = 1_800; // 30 minutes

// Cluster list - advertised to the coordinator during discovery
pub static INPUT_CLUSTERS: [u16; 3] = [
    basic::CLUSTER_ID,
    identify::CLUSTER_ID,
    temperature::CLUSTER_ID,
];
pub static OUTPUT_CLUSTERS: [u16; 0] = [];

pub static ENDPOINT_DESCRIPTOR: EndpointDescriptor = EndpointDescriptor {
    endpoint: ENDPOINT,
    profile_id: profile::HOME_AUTOMATION,
    device_id: 0x0302, // HA temperature sensor
    device_version: 1,
    input_clusters: &INPUT_CLUSTERS,
    output_clusters: &OUTPUT_CLUSTERS,
};

pub static REPORTING: ReportingTable<4> = ReportingTable::new();
pub static TEMPERATURE: TemperatureMeasurementServer =
    TemperatureMeasurementServer::new(MIN_TEMPERATURE, MAX_TEMPERATURE).with_reporting(&REPORTING);

pub struct TemperatureReporter {
    /// Seconds elapsed since boot - caller increments this each tick.
    now_secs: u32,
    /// Timestamp of the last report sent on the unconfigured fallback cadence.
    last_checked_report: u32,
}

impl TemperatureReporter {
    pub const fn new() -> Self {
        Self {
            now_secs: 0,
            last_checked_report: 0,
        }
    }

    pub fn tick(&mut self) {
        self.now_secs = self.now_secs.saturating_add(1);
    }

    /// Returns true if a report is due. If this returns true, the caller should
    pub fn is_due(&mut self) -> bool {
        self.now_secs = self.now_secs.saturating_add(1);
        self.now_secs.saturating_sub(self.last_checked_report) >= DEFAULT_REPORT_INTERVAL_SECS
    }

    pub async fn report_if_temperature_changed<D: ZclSender>(
        &mut self,
        value: i16,
        device: &D,
        seq: u8,
    ) -> Option<i16> {
        self.update(value);
        let cluster = temperature::CLUSTER.id();
        let attribute = temperature::MEASURED_VALUE.id();
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
            .and_then(|frame| frame.push(&temperature::MEASURED_VALUE, Int16(value)))
            .and_then(AttributeReportBuilder::finish)
            .expect("encode temperature report");

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

    fn update(&self, value: i16) {
        TEMPERATURE.set_measured_value(value);
    }
}
