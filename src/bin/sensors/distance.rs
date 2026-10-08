use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    peripherals::{GPIO3, GPIO4},
};

pub struct DistanceSensor {
    trigger: Output<'static>,
    echo: Input<'static>,
}

impl DistanceSensor {
    pub fn new(gpio3: GPIO3<'static>, gpio4: GPIO4<'static>) -> Self {
        let trigger = Output::new(gpio3, Level::Low, OutputConfig::default());
        let echo = Input::new(gpio4, InputConfig::default());

        Self { trigger, echo }
    }

    pub fn read(&mut self, delay: &mut Delay) -> Option<u16> {
        let mut samples = [0u16; 5];
        let mut valid = 0usize;

        for sample in samples.iter_mut() {
            if let Some(cm) = self.read_single(delay) {
                *sample = cm;
                valid += 1;
            }
            // JSN-SR04T needs about 60ms between measurements
            delay.delay_millis(60);
        }

        if valid < 3 {
            return None;
        }

        // Sort the valid readings and get the median
        samples[..valid].sort_unstable();
        Some(samples[valid / 2])
    }

    fn read_single(&mut self, delay: &mut Delay) -> Option<u16> {
        // Send a 10µs trigger pulse
        self.trigger.set_high();
        delay.delay_micros(10);
        self.trigger.set_low();

        // Wait for echo to go high
        let mut timeout = 0u32;
        while self.echo.is_low() {
            timeout += 1;
            if timeout > 30_000 {
                // Sensor is not responding
                return None;
            }
        }

        // Time how long echo stays high
        let mut pulse_us = 0u32;
        while self.echo.is_high() {
            pulse_us += 1;
            delay.delay_micros(1);
            if pulse_us > 38_000 {
                // Out of range
                return None;
            }
        }

        let cm = (pulse_us / 58) as u16;

        // JSN-SR04T minimum reliable range is 20cm
        if cm < 20 {
            return None;
        }

        Some(cm)
    }
}
