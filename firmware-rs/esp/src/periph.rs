//! The peripherals the ASIC drivers use, on ESP-IDF's drivers: GPIO, a UART,
//! the new I2C master driver (the screen's M5GFX uses it too: one driver
//! family, one shared bus) and ADC1 with its calibration.

use std::time::Duration;

use esp_idf_svc::sys::*;

use crate::system::millis;

pub fn delay_ms(ms: u32) {
    std::thread::sleep(Duration::from_millis(ms.max(1) as u64));
}

/// An output pin; its level is written before it becomes an output.
pub fn output(pin: i32, level: bool) {
    unsafe {
        gpio_set_level(pin, level as u32);
        let config =
            gpio_config_t { pin_bit_mask: 1u64 << pin, mode: gpio_mode_t_GPIO_MODE_OUTPUT, ..std::mem::zeroed() };
        gpio_config(&config);
        gpio_set_level(pin, level as u32);
    }
}

/// `pin` as a plain input.
pub fn input(pin: i32) {
    let config =
        gpio_config_t { pin_bit_mask: 1u64 << pin, mode: gpio_mode_t_GPIO_MODE_INPUT, ..unsafe { std::mem::zeroed() } };
    unsafe { gpio_config(&config) };
}

pub fn set(pin: i32, level: bool) {
    unsafe { gpio_set_level(pin, level as u32) };
}

/// A UART to an ASIC, 8N1.
pub struct Uart {
    port: uart_port_t,
    tx: i32,
    rx: i32,
    installed: bool,
}

impl Uart {
    pub fn new(port: uart_port_t, tx: i32, rx: i32) -> Self {
        Uart { port, tx, rx, installed: false }
    }

    pub fn begin(&mut self, baud: u32, rx_buffer: usize) -> bool {
        self.end();
        let config = uart_config_t {
            baud_rate: baud as i32,
            data_bits: uart_word_length_t_UART_DATA_8_BITS,
            parity: uart_parity_t_UART_PARITY_DISABLE,
            stop_bits: uart_stop_bits_t_UART_STOP_BITS_1,
            flow_ctrl: uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
            __bindgen_anon_1: uart_config_t__bindgen_ty_1 {
                source_clk: soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT,
            },
            ..unsafe { std::mem::zeroed() }
        };
        let size = rx_buffer.max(256) as i32;
        unsafe {
            if uart_driver_install(self.port, size, 0, 0, std::ptr::null_mut(), 0) != ESP_OK {
                return false;
            }
            self.installed = true;
            uart_param_config(self.port, &config) == ESP_OK
                && uart_set_pin(self.port, self.tx, self.rx, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE) == ESP_OK
        }
    }

    pub fn end(&mut self) {
        if self.installed {
            unsafe { uart_driver_delete(self.port) };
        }
        self.installed = false;
    }

    pub fn available(&self) -> usize {
        let mut n: usize = 0;
        if !self.installed || unsafe { uart_get_buffered_data_len(self.port, &mut n) } != ESP_OK {
            return 0;
        }
        n
    }

    pub fn read(&self) -> Option<u8> {
        let mut byte = 0u8;
        let n = unsafe { uart_read_bytes(self.port, (&mut byte as *mut u8).cast(), 1, 0) };
        (self.installed && n == 1).then_some(byte)
    }

    /// The whole packet, or false.
    pub fn write(&self, data: &[u8]) -> bool {
        !data.is_empty()
            && self.installed
            && unsafe { uart_write_bytes(self.port, data.as_ptr().cast(), data.len()) } == data.len() as i32
    }

    pub fn flush(&self) {
        if self.installed {
            unsafe { uart_wait_tx_done(self.port, u32::MAX) };
        }
    }

    pub fn set_baud(&self, baud: u32) {
        if self.installed {
            unsafe { uart_set_baudrate(self.port, baud) };
        }
    }

    pub fn drain(&self) {
        while self.read().is_some() {}
    }
}

/// Devices on an I2C bus of the new master driver. The bus is taken over
/// when it is already open (the screen's M5GFX may have opened it first).
pub struct I2c {
    devices: Vec<(u8, i2c_master_dev_handle_t)>,
    bus: i2c_master_bus_handle_t,
}

unsafe impl Send for I2c {}

impl I2c {
    pub fn new(port: i32, sda: i32, scl: i32, addresses: &[u8]) -> Option<Self> {
        let mut bus: i2c_master_bus_handle_t = std::ptr::null_mut();
        unsafe {
            // A port nobody opened yet is the usual case: not an error
            // worth ESP-IDF's red log line.
            esp_log_level_set(c"i2c.master".as_ptr(), esp_log_level_t_ESP_LOG_NONE);
            let existing = i2c_master_get_bus_handle(port, &mut bus) == ESP_OK && !bus.is_null();
            esp_log_level_set(c"i2c.master".as_ptr(), esp_log_level_t_ESP_LOG_INFO);
            if !existing {
                let mut config: i2c_master_bus_config_t = std::mem::zeroed();
                config.i2c_port = port;
                config.sda_io_num = sda;
                config.scl_io_num = scl;
                config.__bindgen_anon_1.clk_source = soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT;
                config.glitch_ignore_cnt = 7;
                config.flags.set_enable_internal_pullup(1);
                if i2c_new_master_bus(&config, &mut bus) != ESP_OK {
                    return None;
                }
            }
        }
        let mut devices = Vec::new();
        for &address in addresses {
            let mut config: i2c_device_config_t = unsafe { std::mem::zeroed() };
            config.dev_addr_length = i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7;
            config.device_address = address as u16;
            config.scl_speed_hz = 400_000;
            let mut dev: i2c_master_dev_handle_t = std::ptr::null_mut();
            if unsafe { i2c_master_bus_add_device(bus, &config, &mut dev) } != ESP_OK {
                return None;
            }
            devices.push((address, dev));
        }
        Some(I2c { devices, bus })
    }

    fn device(&self, address: u8) -> Option<i2c_master_dev_handle_t> {
        self.devices.iter().find(|(a, _)| *a == address).map(|(_, d)| *d)
    }

    pub fn write(&self, address: u8, bytes: &[u8]) -> bool {
        let Some(dev) = self.device(address) else { return false };
        unsafe { i2c_master_transmit(dev, bytes.as_ptr(), bytes.len(), 50) == ESP_OK }
    }

    pub fn read(&self, address: u8, reg: u8, out: &mut [u8]) -> bool {
        let Some(dev) = self.device(address) else { return false };
        unsafe { i2c_master_transmit_receive(dev, &reg, 1, out.as_mut_ptr(), out.len(), 50) == ESP_OK }
    }

    pub fn bus(&self) -> i2c_master_bus_handle_t {
        self.bus
    }
}

/// One ADC1 pin, calibrated millivolts (as Arduino's analogReadMilliVolts):
/// the NerdNOS NTC (ESP32-S3, curve fitting) and the M5StickC Plus2's
/// battery (ESP32, line fitting).
#[cfg(any(feature = "nerdnos", feature = "m5stickc-plus2"))]
pub struct Adc {
    unit: adc_oneshot_unit_handle_t,
    channel: adc_channel_t,
    cali: adc_cali_handle_t,
}

#[cfg(any(feature = "nerdnos", feature = "m5stickc-plus2"))]
unsafe impl Send for Adc {}

#[cfg(any(feature = "nerdnos", feature = "m5stickc-plus2"))]
impl Adc {
    pub fn new(pin: i32) -> Option<Self> {
        unsafe {
            let mut unit_id: adc_unit_t = 0;
            let mut channel: adc_channel_t = 0;
            if adc_oneshot_io_to_channel(pin, &mut unit_id, &mut channel) != ESP_OK || unit_id != adc_unit_t_ADC_UNIT_1
            {
                return None;
            }
            let init = adc_oneshot_unit_init_cfg_t { unit_id, ..std::mem::zeroed() };
            let mut unit: adc_oneshot_unit_handle_t = std::ptr::null_mut();
            if adc_oneshot_new_unit(&init, &mut unit) != ESP_OK {
                return None;
            }
            let chan = adc_oneshot_chan_cfg_t {
                atten: adc_atten_t_ADC_ATTEN_DB_12,
                bitwidth: adc_bitwidth_t_ADC_BITWIDTH_DEFAULT,
            };
            if adc_oneshot_config_channel(unit, channel, &chan) != ESP_OK {
                return None;
            }
            let mut cali: adc_cali_handle_t = std::ptr::null_mut();
            #[cfg(not(esp32))]
            {
                let cali_config = adc_cali_curve_fitting_config_t {
                    unit_id,
                    chan: channel,
                    atten: adc_atten_t_ADC_ATTEN_DB_12,
                    bitwidth: adc_bitwidth_t_ADC_BITWIDTH_DEFAULT,
                };
                if adc_cali_create_scheme_curve_fitting(&cali_config, &mut cali) != ESP_OK {
                    cali = std::ptr::null_mut();
                }
            }
            // The original ESP32 has line fitting only (eFuse Vref or Two
            // Point values, else the nominal 1100 mV).
            #[cfg(esp32)]
            {
                let cali_config = adc_cali_line_fitting_config_t {
                    unit_id,
                    atten: adc_atten_t_ADC_ATTEN_DB_12,
                    bitwidth: adc_bitwidth_t_ADC_BITWIDTH_DEFAULT,
                    default_vref: 1100,
                };
                if adc_cali_create_scheme_line_fitting(&cali_config, &mut cali) != ESP_OK {
                    cali = std::ptr::null_mut();
                }
            }
            Some(Adc { unit, channel, cali })
        }
    }

    pub fn millivolts(&self) -> u32 {
        let mut raw = 0i32;
        if unsafe { adc_oneshot_read(self.unit, self.channel, &mut raw) } != ESP_OK {
            return 0;
        }
        let mut mv = 0i32;
        if !self.cali.is_null() && unsafe { adc_cali_raw_to_voltage(self.cali, raw, &mut mv) } == ESP_OK {
            return mv.max(0) as u32;
        }
        (raw.max(0) as u32) * 3100 / 4095
    }
}

/// Milliseconds since boot, as the C++ firmware's millis().
pub fn now() -> u32 {
    millis()
}
