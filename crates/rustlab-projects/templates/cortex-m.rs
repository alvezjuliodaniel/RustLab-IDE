#![no_std]
#![no_main]

use core::panic::PanicInfo;
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn halt(_: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
