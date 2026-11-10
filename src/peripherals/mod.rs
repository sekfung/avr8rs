pub(crate) mod adc;
pub(crate) mod clock;
pub(crate) mod eeprom;
pub(crate) mod gpio;
pub(crate) mod spi;
pub(crate) mod timer;
pub(crate) mod timer_attiny;
pub(crate) mod twi;
pub(crate) mod usart;

pub use adc::{
    adc_config, atmega328_channels, Adc, AdcConfig, AdcMuxConfiguration, AdcMuxInput,
    AdcMuxInputType, AdcReference, AdcState,
};
pub use clock::{clock_config, Clock, ClockConfig};
pub use eeprom::{eeprom_config, Eeprom, EepromBackend, EepromConfig, EepromMemoryBackend};
pub use gpio::{
    port_a_config, port_b_config, port_c_config, port_d_config, port_e_config, port_f_config,
    port_g_config, port_h_config, port_j_config, port_k_config, port_l_config,
    ExternalClockListener, ExternalInterrupt, GpioListener, IoPort, ListenerId, PinChangeInterrupt,
    PinOverrideMode, PinState, PortConfig, INT0, INT1, PCINT0, PCINT1, PCINT2,
};
pub use spi::{spi_config, Spi, SpiConfig, SpiTransferCallback};
pub use timer::{timer0_config, timer1_config, timer2_config, Timer, TimerConfig};
pub use timer_attiny::{attiny_timer1_config, AttinyTimer1, AttinyTimer1Config};
pub use twi::{twi_config, NoopTwiEventHandler, Twi, TwiConfig, TwiContext, TwiEventHandler};
pub use usart::{usart0_config, Usart, UsartConfig};
