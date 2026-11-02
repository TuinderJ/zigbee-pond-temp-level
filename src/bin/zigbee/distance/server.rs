use core::sync::atomic::{AtomicU16, Ordering};

use zigbee::{
    zcl::{
        clusters::general::analog_input,
        frame::Status,
        reporting::AttributeReporting,
        server::ClusterServer,
        types::{
            AttrInfo, Attribute, AttributeId, Cluster, ClusterId, ReadOnly, Reportable, Uint16,
        },
    },
    zdo::{ClusterReply, ClusterRequest, ClusterRequestHandler},
};

pub const CLUSTER: Cluster = Cluster::new(ClusterId(analog_input::CLUSTER_ID), "AnalogInput");
pub const PRESENT_VALUE: Attribute<Uint16, ReadOnly, Reportable> = CLUSTER.attribute(
    AttributeId(analog_input::attribute::PRESENT_VALUE),
    "PresentValue",
);
const ATTRIBUTES: &[AttrInfo] = &[PRESENT_VALUE.attr_info()];

pub struct AnalogInputServer<'a> {
    present_value: AtomicU16,
    reporting: Option<&'a dyn AttributeReporting>,
}

impl<'a> AnalogInputServer<'a> {
    pub const fn new() -> Self {
        Self {
            present_value: AtomicU16::new(0),
            reporting: None,
        }
    }

    #[must_use]
    pub const fn with_reporting(mut self, store: &'a dyn AttributeReporting) -> Self {
        self.reporting = Some(store);
        self
    }

    pub fn present_value(&self) -> u16 {
        self.present_value.load(Ordering::Relaxed)
    }

    pub fn set_present_value(&self, value: u16) {
        self.present_value.store(value, Ordering::Relaxed);
    }
}

impl ClusterServer for AnalogInputServer<'_> {
    fn cluster(&self) -> Cluster {
        CLUSTER
    }

    fn attributes(&self) -> &'static [AttrInfo] {
        ATTRIBUTES
    }

    fn encode_value(
        &self,
        id: AttributeId,
        out: &mut [u8],
        offset: &mut usize,
    ) -> zigbee::zcl::frame::Status {
        let encoded = match id.0 {
            attribute::PRESENT_VALUE => {
                PRESENT_VALUE.encode(Uint16(self.present_value()), out, offset)
            }
            _ => return Status::UnsupportedAttribute,
        };

        match encoded {
            Ok(()) => Status::Success,
            Err(_) => Status::InsufficientSpace,
        }
    }
}

impl ClusterRequestHandler for AnalogInputServer<'_> {
    fn handle(&self, request: &ClusterRequest<'_>, out: &mut [u8]) -> Option<ClusterReply> {
        self.handle_request(request, out)
    }
}

/// Bare attribute identifiers, for matching against a received record.
///
/// Derived from the descriptors above, which stay the source of truth for the
/// identifier and its type.
pub mod attribute {
    /// `MeasuredValue` (`Int16`, hundredths of a degree Celsius).
    pub const PRESENT_VALUE: u16 = super::PRESENT_VALUE.id().0;
    // /// `MinMeasuredValue` (`Int16`).
    // pub const MIN_MEASURED_VALUE: u16 = super::MIN_MEASURED_VALUE.id().0;
    // /// `MaxMeasuredValue` (`Int16`).
    // pub const MAX_MEASURED_VALUE: u16 = super::MAX_MEASURED_VALUE.id().0;
}
