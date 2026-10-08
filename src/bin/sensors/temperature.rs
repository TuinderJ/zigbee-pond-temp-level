use esp_hal::{
    delay::Delay,
    gpio::{DriveMode, Flex, OutputConfig},
    peripherals::GPIO2,
};

enum Command {
    SkipRom = 0xCC,
    ConvertTemp = 0x44,
    ReadScratchpad = 0xBE,
}

pub struct TemperatureSensor {
    pin: Flex<'static>,
}

impl TemperatureSensor {
    pub fn new(gpio2: GPIO2<'static>) -> Self {
        let mut pin = Flex::new(gpio2);
        pin.apply_output_config(&OutputConfig::default().with_drive_mode(DriveMode::OpenDrain));
        Self { pin }
    }

    /// Returns temperature in hundredths of a degree Celsius (Zigbee Standard).
    /// e.g. 25.00°C → 2500. Returns None if the sensor doesn't respond.
    pub fn read(&mut self, delay: &mut Delay) -> Option<i16> {
        // Step 1: Reset pulse - tells the sensor we're starting a transaction
        self.reset(delay)?;

        // Step 2: Tx | 0xCC | Skip ROM | There is only 1 device on the bus se we skip ROM
        self.write_byte(Command::SkipRom as u8, delay);

        // Step 3: Tx | 0x44 | Convert T | Tell the ds18b20 to measure the current temperature
        self.write_byte(Command::ConvertTemp as u8, delay);

        // Step 4: Temperature Conversion takes 750 ms
        delay.delay_millis(750);

        // Step 5: Reset pulse
        self.reset(delay)?;

        // Step 6: Tx | 0xCC | Skip ROM
        self.write_byte(Command::SkipRom as u8, delay);

        // Step 7: Tx | 0xBE | Read scratchpad | tells the ds18b20 to emit it's scratchpad bytes
        self.write_byte(Command::ReadScratchpad as u8, delay);

        // Step 8: Rx | Read 2 bytes from the scratchpad
        // The temperature is stored in these 2 bytes
        let lsb = self.read_byte(delay);
        let msb = self.read_byte(delay);

        // Step 9: Combine the bytes into an i16 and convert them.
        // Raw value is in units of 1/16°C
        let raw = i16::from_le_bytes([lsb, msb]);
        // raw / 16 * 100 = ZigBee Standard
        let zigbee_temp = ((raw as i32 * 100) / 16) as i16;
        Some(zigbee_temp)
    }

    /// Sends a reset pulse and listens for a presence pulse from the sensor.
    /// Returns None if no device responds.
    fn reset(&mut self, delay: &mut Delay) -> Option<()> {
        // hold low for 480µs for reset pulse
        self.pin.set_low();
        delay.delay_micros(480);
        // wait for presence pulse window
        self.pin.set_high();
        delay.delay_micros(70);

        // the ds18b20 will pull the pin low to transmit it's presence
        let present = self.pin.is_low();
        // remainder of time required to let the bus recover
        delay.delay_micros(410);

        if present { Some(()) } else { None }
    }

    /// Writes a byte on the one-wire bus.
    fn write_byte(&mut self, mut byte: u8, delay: &mut Delay) {
        // Cycle through each bit in the byte
        for _ in 0..8 {
            if byte & 0x01 == 1 {
                // Write 1: pull low for 6µs, release for 64µs
                self.pin.set_low();
                delay.delay_micros(6);
                self.pin.set_high();
                delay.delay_micros(64);
            } else {
                // Write 0: pull low for 60µs, release for 10µs
                self.pin.set_low();
                delay.delay_micros(60);
                self.pin.set_high();
                delay.delay_micros(10);
            }
            byte >>= 1;
        }
    }

    /// Reads one byte, LSB first.
    fn read_byte(&mut self, delay: &mut Delay) -> u8 {
        let mut byte = 0u8;
        for i in 0..8 {
            // Initiate the read slot
            self.pin.set_low();
            delay.delay_micros(3);
            self.pin.set_high();
            // Wait for the ds18b20 to send its bit
            delay.delay_micros(10);

            if self.pin.is_high() {
                byte |= 1 << i;
            }
        }
        byte
    }
}
