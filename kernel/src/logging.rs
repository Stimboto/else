use uart_16550::SerialPort;
use core::fmt;
use core::fmt::Write;

static mut SERIAL_PORT: Option<SerialPort> = None;

// Minimal logger implementation
struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // SAFE: We are single-threaded at this phase.
            unsafe {
                if let Some(port) = &mut *(&raw mut SERIAL_PORT) {
                    writeln!(port, "[{}] {}", record.level(), record.args()).unwrap();
                }
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: KernelLogger = KernelLogger;

pub fn init() {
    // SAFE: COM1 is standardly at 0x3F8, initializing the port is safe here
    // as no other part of the system is using it yet.
    let mut serial_port = unsafe { SerialPort::new(0x3F8) };
    serial_port.init();
    
    unsafe {
        SERIAL_PORT = Some(serial_port);
    }
    
    // SAFE: We just initialized the static LOGGER.
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // SAFE: Single-threaded phase.
    unsafe {
        if let Some(port) = &mut *(&raw mut SERIAL_PORT) {
            port.write_fmt(args).unwrap();
        }
    }
}
