use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::cpu::{Cpu, CpuState, InterruptConfig};

pub(crate) type PendingPinSets = RefCell<VecDeque<(IoPort, u8, bool)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalInterrupt {
    pub eicr: u16,
    pub eimsk: u16,
    pub eifr: u16,
    pub isc_offset: u8,
    pub index: u8,
    pub interrupt: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinChangeInterrupt {
    pub pcie: u8,
    pub pcicr: u16,
    pub pcifr: u16,
    pub pcmsk: u16,
    pub pin_change_interrupt: u8,
    pub mask: u8,
    pub offset: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortConfig {
    pub pin: u16,
    pub ddr: u16,
    pub port: u16,
    pub pin_change: Option<PinChangeInterrupt>,
    pub external_interrupts: Vec<Option<ExternalInterrupt>>,
}

pub const INT0: ExternalInterrupt = ExternalInterrupt {
    eicr: 0x69,
    eimsk: 0x3d,
    eifr: 0x3c,
    index: 0,
    isc_offset: 0,
    interrupt: 2,
};

pub const INT1: ExternalInterrupt = ExternalInterrupt {
    eicr: 0x69,
    eimsk: 0x3d,
    eifr: 0x3c,
    index: 1,
    isc_offset: 2,
    interrupt: 4,
};

pub const PCINT0: PinChangeInterrupt = PinChangeInterrupt {
    pcie: 0,
    pcicr: 0x68,
    pcifr: 0x3b,
    pcmsk: 0x6b,
    pin_change_interrupt: 6,
    mask: 0xff,
    offset: 0,
};

pub const PCINT1: PinChangeInterrupt = PinChangeInterrupt {
    pcie: 1,
    pcicr: 0x68,
    pcifr: 0x3b,
    pcmsk: 0x6c,
    pin_change_interrupt: 8,
    mask: 0xff,
    offset: 0,
};

pub const PCINT2: PinChangeInterrupt = PinChangeInterrupt {
    pcie: 2,
    pcicr: 0x68,
    pcifr: 0x3b,
    pcmsk: 0x6d,
    pin_change_interrupt: 10,
    mask: 0xff,
    offset: 0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PinState {
    Low = 0,
    High = 1,
    Input = 2,
    InputPullUp = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PinOverrideMode {
    None = 0,
    Enable = 1,
    Set = 2,
    Clear = 3,
    Toggle = 4,
}

pub type ListenerId = u64;
pub type GpioListener = Box<dyn FnMut(&mut CpuState, u8, u8)>;

type ListenerCell = Rc<RefCell<GpioListener>>;
pub type ExternalClockListener = Box<dyn FnMut(&mut CpuState, bool)>;
type ExternalClockCell = Rc<RefCell<ExternalClockListener>>;

pub fn port_a_config() -> PortConfig {
    PortConfig {
        pin: 0x20,
        ddr: 0x21,
        port: 0x22,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_b_config() -> PortConfig {
    PortConfig {
        pin: 0x23,
        ddr: 0x24,
        port: 0x25,
        pin_change: Some(PCINT0),
        external_interrupts: Vec::new(),
    }
}

pub fn port_c_config() -> PortConfig {
    PortConfig {
        pin: 0x26,
        ddr: 0x27,
        port: 0x28,
        pin_change: Some(PCINT1),
        external_interrupts: Vec::new(),
    }
}

pub fn port_d_config() -> PortConfig {
    PortConfig {
        pin: 0x29,
        ddr: 0x2a,
        port: 0x2b,
        pin_change: Some(PCINT2),
        external_interrupts: vec![None, None, Some(INT0), Some(INT1)],
    }
}

pub fn port_e_config() -> PortConfig {
    PortConfig {
        pin: 0x2c,
        ddr: 0x2d,
        port: 0x2e,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_f_config() -> PortConfig {
    PortConfig {
        pin: 0x2f,
        ddr: 0x30,
        port: 0x31,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_g_config() -> PortConfig {
    PortConfig {
        pin: 0x32,
        ddr: 0x33,
        port: 0x34,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_h_config() -> PortConfig {
    PortConfig {
        pin: 0x100,
        ddr: 0x101,
        port: 0x102,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_j_config() -> PortConfig {
    PortConfig {
        pin: 0x103,
        ddr: 0x104,
        port: 0x105,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_k_config() -> PortConfig {
    PortConfig {
        pin: 0x106,
        ddr: 0x107,
        port: 0x108,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

pub fn port_l_config() -> PortConfig {
    PortConfig {
        pin: 0x109,
        ddr: 0x10a,
        port: 0x10b,
        pin_change: None,
        external_interrupts: Vec::new(),
    }
}

struct IoPortState {
    cpu: Weak<RefCell<CpuState>>,
    config: PortConfig,
    external_ints: Vec<Option<InterruptConfig>>,
    pcint: Option<InterruptConfig>,
    listeners: Vec<(ListenerId, ListenerCell)>,
    next_listener_id: ListenerId,
    pin_value: u8,
    driven_pins: u8,
    override_mask: u8,
    override_value: u8,
    last_value: u8,
    last_ddr: u8,
    last_pin: u8,
    port_value: u8,
    open_collector: u8,
    external_clock_listeners: [Option<ExternalClockCell>; 8],
    pending_external_clock: Vec<(u8, bool)>,
    pending_pin_sets: Weak<PendingPinSets>,
}

impl IoPortState {
    fn new(cpu: Cpu, config: PortConfig) -> Self {
        let external_ints = config
            .external_interrupts
            .iter()
            .map(|external| {
                external.map(|external| InterruptConfig {
                    address: external.interrupt,
                    enable_register: external.eimsk,
                    enable_mask: 1 << external.index,
                    flag_register: external.eifr,
                    flag_mask: 1 << external.index,
                    constant: false,
                    inverse_flag: false,
                })
            })
            .collect();
        let pcint = config.pin_change.map(|pin_change| InterruptConfig {
            address: pin_change.pin_change_interrupt,
            enable_register: pin_change.pcicr,
            enable_mask: 1 << pin_change.pcie,
            flag_register: pin_change.pcifr,
            flag_mask: 1 << pin_change.pcie,
            constant: false,
            inverse_flag: false,
        });

        Self {
            cpu: cpu.downgrade(),
            config,
            external_ints,
            pcint,
            listeners: Vec::new(),
            next_listener_id: 1,
            pin_value: 0,
            driven_pins: 0,
            override_mask: 0xff,
            override_value: 0,
            last_value: 0,
            last_ddr: 0,
            last_pin: 0,
            port_value: 0,
            open_collector: 0,
            external_clock_listeners: std::array::from_fn(|_| None),
            pending_external_clock: Vec::new(),
            pending_pin_sets: Rc::downgrade(&cpu.borrow_state_mut().pending_pin_sets),
        }
    }

    fn toggle_external_interrupt(&mut self, state: &mut CpuState, pin: u8, rising: bool) {
        let Some(external_config) = self
            .config
            .external_interrupts
            .get(pin as usize)
            .copied()
            .flatten()
        else {
            return;
        };
        let Some(mut external) = self.external_ints.get(pin as usize).copied().flatten() else {
            return;
        };
        if state.data_byte(external_config.eimsk) & (1 << external_config.index) == 0 {
            return;
        }

        let configuration =
            (state.data_byte(external_config.eicr) >> external_config.isc_offset) & 0x3;
        let (generate_interrupt, constant) = match configuration {
            0 => (!rising, true),
            1 => (true, false),
            2 => (!rising, false),
            3 => (rising, false),
            _ => unreachable!(),
        };
        external.constant = constant;
        self.external_ints[pin as usize] = Some(external);

        if generate_interrupt {
            state.set_interrupt_flag(external);
        } else if external.constant {
            state.clear_interrupt(external);
        }
    }

    fn check_external_interrupts(&self, state: &mut CpuState) {
        for (pin, external_config) in self.config.external_interrupts.iter().enumerate() {
            let Some(external_config) = external_config else {
                continue;
            };
            if state.data_byte(external_config.eimsk) & (1 << external_config.index) == 0
                || self.last_pin & (1 << pin) != 0
            {
                continue;
            }
            let configuration =
                (state.data_byte(external_config.eicr) >> external_config.isc_offset) & 0x3;
            if configuration == 0 {
                if let Some(mut external) = self.external_ints[pin] {
                    external.constant = true;
                    state.queue_interrupt(external);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct IoPort(Rc<RefCell<IoPortState>>);

impl IoPort {
    pub fn new(cpu: Cpu, config: PortConfig) -> Self {
        let port = Self(Rc::new(RefCell::new(IoPortState::new(cpu, config))));
        port.register_with_cpu();
        port.install_register_hooks();
        port.install_external_interrupt_hooks();
        port.install_pin_change_hooks();
        port
    }

    pub fn add_listener(&self, listener: GpioListener) -> ListenerId {
        let mut state = self.0.borrow_mut();
        let id = state.next_listener_id;
        state.next_listener_id += 1;
        state.listeners.push((id, Rc::new(RefCell::new(listener))));
        id
    }

    pub fn remove_listener(&self, id: ListenerId) {
        self.0
            .borrow_mut()
            .listeners
            .retain(|(listener_id, _)| *listener_id != id);
    }

    pub fn set_pin(&self, index: u8, value: bool) {
        let cpu = self.cpu();
        let mut state = match cpu.try_borrow_state_mut() {
            Ok(state) => state,
            Err(_) => {
                self.0
                    .borrow()
                    .pending_pin_sets
                    .upgrade()
                    .expect("CPU owns the GPIO input queue")
                    .borrow_mut()
                    .push_back((self.clone(), index, value));
                return;
            }
        };
        state.drain_pending_pin_sets();
        self.set_pin_with(&mut state, index, value);
        state.drain_pending_pin_sets();
    }

    pub(crate) fn set_pin_with(&self, state: &mut CpuState, index: u8, value: bool) {
        {
            let mut port_state = self.0.borrow_mut();
            let bit_mask = 1 << index;
            port_state.driven_pins |= bit_mask;
            port_state.pin_value &= !bit_mask;
            if value {
                port_state.pin_value |= bit_mask;
            }
        }
        let ddr = state.data_byte(self.0.borrow().config.ddr);
        self.refresh_pin_register(state, ddr);
    }

    pub(crate) fn reset_with_state(&self, state: &mut CpuState) {
        {
            let mut port = self.0.borrow_mut();
            port.override_mask = 0xff;
            port.override_value = 0;
            port.pending_external_clock.clear();
        }
        self.apply_write(state, 0, 0);
    }

    pub fn pin_state(&self, index: u8) -> PinState {
        let state = self.0.borrow();
        let bit_mask = 1u8 << index;
        let open_state = if state.port_value & bit_mask != 0 {
            PinState::InputPullUp
        } else {
            PinState::Input
        };
        let high_value = if state.open_collector & bit_mask != 0 {
            open_state
        } else {
            PinState::High
        };
        if state.last_ddr & bit_mask != 0 {
            if state.last_value & bit_mask != 0 {
                high_value
            } else {
                PinState::Low
            }
        } else {
            open_state
        }
    }

    pub fn timer_override_pin(&self, index: u8, mode: PinOverrideMode) {
        let cpu = self.cpu();
        let mut state = cpu.borrow_state_mut();
        self.timer_override_pin_with(&mut state, index, mode);
        state.drain_pending_pin_sets();
    }

    pub(crate) fn timer_override_pin_with(
        &self,
        state: &mut CpuState,
        index: u8,
        mode: PinOverrideMode,
    ) {
        let port_addr = self.0.borrow().config.port;
        let port_value = state.data_byte(port_addr);
        self.0.borrow_mut().update_override(index, mode, port_value);
        let ddr = state.data_byte(self.0.borrow().config.ddr);
        self.apply_write(state, port_value, ddr);
    }

    pub fn set_open_collector(&self, value: u8) {
        self.0.borrow_mut().open_collector = value;
    }

    pub fn set_external_clock_listener(
        &self,
        index: usize,
        listener: Option<ExternalClockListener>,
    ) {
        if index < 8 {
            self.0.borrow_mut().external_clock_listeners[index] =
                listener.map(|listener| Rc::new(RefCell::new(listener)));
        }
    }

    fn cpu(&self) -> Cpu {
        Cpu::from_weak(&self.0.borrow().cpu)
            .expect("CPU was dropped while its GPIO port is still in use")
    }

    fn register_with_cpu(&self) {
        let cpu = self.cpu();
        let port_addr = self.0.borrow().config.port;
        let mut state = cpu.borrow_state_mut();
        state.gpio_ports.push(self.clone());
        let index = port_addr as usize;
        if state.gpio_by_port.len() <= index {
            state.gpio_by_port.resize(index + 1, None);
        }
        state.gpio_by_port[index] = Some(self.clone());
    }

    fn install_register_hooks(&self) {
        let cpu = self.cpu();
        let (ddr_addr, port_addr, pin_addr) = {
            let state = self.0.borrow();
            (state.config.ddr, state.config.port, state.config.pin)
        };

        let port = self.clone();
        cpu.set_write_hook(
            ddr_addr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                let port_value = state.data_byte(port_addr);
                state.set_data_byte(ddr_addr, value);
                port.apply_write(state, port_value, value);
                true
            }),
        );

        let port = self.clone();
        cpu.set_write_hook(
            port_addr,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                let ddr = state.data_byte(ddr_addr);
                state.set_data_byte(port_addr, value);
                port.apply_write(state, value, ddr);
                true
            }),
        );

        let port = self.clone();
        cpu.set_write_hook(
            pin_addr,
            Box::new(move |state, value, _old_value, _addr, mask| {
                let old_port_value = state.data_byte(port_addr);
                let ddr = state.data_byte(ddr_addr);
                let port_value = old_port_value ^ (value & mask);
                state.set_data_byte(port_addr, port_value);
                port.apply_write(state, port_value, ddr);
                true
            }),
        );
    }

    fn install_external_interrupt_hooks(&self) {
        let cpu = self.cpu();
        let config = self.0.borrow().config.clone();
        for external in config.external_interrupts.iter().flatten() {
            Self::attach_interrupt_hook(&cpu, external.eicr, RegisterKind::Other);
            Self::attach_interrupt_hook(&cpu, external.eimsk, RegisterKind::Mask);
            Self::attach_interrupt_hook(&cpu, external.eifr, RegisterKind::Flag);
        }
    }

    fn attach_interrupt_hook(cpu: &Cpu, register: u16, kind: RegisterKind) {
        cpu.set_write_hook(
            register,
            Box::new(move |state, value, _old_value, _addr, _mask| {
                if kind != RegisterKind::Flag {
                    state.set_data_byte(register, value);
                }
                let ports = state.gpio_ports.clone();
                for port in ports {
                    port.handle_external_register_write(state, kind, value);
                }
                true
            }),
        );
    }

    fn handle_external_register_write(&self, state: &mut CpuState, kind: RegisterKind, value: u8) {
        let external_ints: Vec<InterruptConfig> = self
            .0
            .borrow()
            .external_ints
            .iter()
            .flatten()
            .copied()
            .collect();
        for external in external_ints {
            match kind {
                RegisterKind::Mask => state.update_interrupt_enable(external, value),
                RegisterKind::Flag if !external.constant => {
                    state.clear_interrupt_by_flag(external, value)
                }
                RegisterKind::Other | RegisterKind::Flag => {}
            }
        }
        self.0.borrow().check_external_interrupts(state);
    }

    fn install_pin_change_hooks(&self) {
        let cpu = self.cpu();
        let Some(pin_change) = self.0.borrow().config.pin_change else {
            return;
        };

        cpu.set_write_hook(
            pin_change.pcifr,
            Box::new(move |state, value, _old_value, addr, mask| {
                let ports = state.gpio_ports.clone();
                for port in ports {
                    if let Some(pcint) = port.0.borrow().pcint.filter(|p| p.flag_register == addr) {
                        state.clear_interrupt_by_flag(pcint, value & mask);
                    }
                }
                true
            }),
        );

        cpu.set_write_hook(
            pin_change.pcicr,
            Box::new(move |state, value, _old_value, addr, _mask| {
                state.set_data_byte(addr, value);
                let ports = state.gpio_ports.clone();
                for port in ports {
                    if let Some(pcint) = port.0.borrow().pcint.filter(|p| p.enable_register == addr)
                    {
                        state.update_interrupt_enable(pcint, value);
                    }
                }
                true
            }),
        );
    }

    fn apply_write(&self, state: &mut CpuState, port_value: u8, ddr: u8) {
        let (new_value, previous_value, changed) = {
            let mut port_state = self.0.borrow_mut();
            port_state.port_value = port_value;
            let new_value = (((port_value & port_state.override_mask) | port_state.override_value)
                & ddr)
                | (port_value & !ddr);
            let previous_value = port_state.last_value;
            let changed = new_value != previous_value || ddr != port_state.last_ddr;
            port_state.last_value = new_value;
            port_state.last_ddr = ddr;
            (new_value, previous_value, changed)
        };

        if changed {
            self.notify_listeners(state, new_value, previous_value);
        }
        self.refresh_pin_register(state, ddr);
    }

    fn notify_listeners(&self, state: &mut CpuState, value: u8, old_value: u8) {
        let listeners: Vec<ListenerCell> = self
            .0
            .borrow()
            .listeners
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        for listener in listeners {
            listener.borrow_mut()(state, value, old_value);
        }
    }

    fn refresh_pin_register(&self, state: &mut CpuState, ddr: u8) {
        let (pin_addr, new_pin, transitions) = {
            let mut port_state = self.0.borrow_mut();
            let pin_addr = port_state.config.pin;
            let (new_pin, transitions) = port_state.recompute_pin(ddr);
            (pin_addr, new_pin, transitions)
        };
        state.set_data_byte(pin_addr, new_pin);

        for (index, rising) in transitions {
            self.toggle_interrupt(state, index, rising);
            self.notify_external_clock_listener(state, index, rising);
        }
        self.0.borrow_mut().last_pin = new_pin;
    }

    fn toggle_interrupt(&self, state: &mut CpuState, index: u8, rising: bool) {
        self.0
            .borrow_mut()
            .toggle_external_interrupt(state, index, rising);

        let pcint = {
            let port_state = self.0.borrow();
            match (port_state.config.pin_change, port_state.pcint) {
                (Some(pin_change), Some(pcint))
                    if pin_change.mask & (1 << index) != 0
                        && state.data_byte(pin_change.pcmsk)
                            & (1 << (index + pin_change.offset))
                            != 0 =>
                {
                    Some(pcint)
                }
                _ => None,
            }
        };
        if let Some(pcint) = pcint {
            state.set_interrupt_flag(pcint);
        }
    }

    fn notify_external_clock_listener(&self, state: &mut CpuState, index: u8, value: bool) {
        let listener = self.0.borrow().external_clock_listeners[index as usize].clone();
        if let Some(listener) = listener {
            match listener.try_borrow_mut() {
                Ok(mut callback) => {
                    callback(state, value);
                    drop(callback);
                    self.drain_pending_external_clock(state);
                }
                Err(_) => self
                    .0
                    .borrow_mut()
                    .pending_external_clock
                    .push((index, value)),
            }
        }
    }

    fn drain_pending_external_clock(&self, state: &mut CpuState) {
        loop {
            let pending = self.0.borrow_mut().pending_external_clock.pop();
            match pending {
                Some((index, value)) => self.notify_external_clock_listener(state, index, value),
                None => break,
            }
        }
    }
}

impl IoPortState {
    fn update_override(&mut self, index: u8, mode: PinOverrideMode, port_value: u8) {
        let pin_mask = 1u8 << index;
        if mode == PinOverrideMode::None {
            self.override_mask |= pin_mask;
            self.override_value &= !pin_mask;
            return;
        }

        self.override_mask &= !pin_mask;
        match mode {
            PinOverrideMode::None => unreachable!(),
            PinOverrideMode::Enable => {
                self.override_value &= !pin_mask;
                self.override_value |= port_value & pin_mask;
            }
            PinOverrideMode::Set => self.override_value |= pin_mask,
            PinOverrideMode::Clear => self.override_value &= !pin_mask,
            PinOverrideMode::Toggle => self.override_value ^= pin_mask,
        }
    }

    fn recompute_pin(&mut self, ddr: u8) -> (u8, Vec<(u8, bool)>) {
        let inputs = self.pin_value | (self.port_value & !self.driven_pins);
        let new_pin = (inputs & !ddr) | (self.last_value & ddr);
        let old_pin = self.last_pin;
        let mut transitions = Vec::new();
        if old_pin != new_pin {
            for index in 0..8 {
                let bit_mask = 1u8 << index;
                if (new_pin & bit_mask) != (old_pin & bit_mask) {
                    transitions.push((index, new_pin & bit_mask != 0));
                }
            }
        }
        (new_pin, transitions)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegisterKind {
    Other,
    Mask,
    Flag,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{asm_program, Recorder, TestProgramRunner};

    const PINB: u16 = 0x23;
    const DDRB: u16 = 0x24;
    const PORTB: u16 = 0x25;
    const EIFR: u16 = 0x3c;
    const EIMSK: u16 = 0x3d;
    const PCICR: u16 = 0x68;
    const EICRA: u16 = 0x69;
    const PCIFR: u16 = 0x3b;
    const PCMSK0: u16 = 0x6b;
    const INT0_BIT: u8 = 0;
    const ISC00: u8 = 0;
    const ISC01: u8 = 1;
    const PCIE0: u8 = 0;
    const PCINT3: u8 = 3;
    const PB0: u8 = 0;
    const PB1: u8 = 1;
    const PB3: u8 = 3;
    const PB4: u8 = 4;
    const PD2: u8 = 2;
    const PC_INT_INT0: u32 = 2;
    const PC_INT_PCINT0: u32 = 6;

    fn record_listener(port: &IoPort) -> Recorder<(u8, u8)> {
        let calls = Recorder::<(u8, u8)>::new();
        let recorder = calls.clone();
        port.add_listener(Box::new(move |_state, value, old_value| {
            recorder.push((value, old_value))
        }));
        calls
    }

    #[test]
    fn should_invoke_the_listeners_when_the_port_is_written_to() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0x0f);
        let calls = record_listener(&port);
        cpu.write_data(PORTB, 0x55);
        assert_eq!(calls.calls(), vec![(0x55, 0)]);
        assert_eq!(cpu.data_byte(PINB), 0x55); // High outputs and undriven pull-ups.
    }

    #[test]
    fn should_invoke_the_listeners_when_ddr_changes() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(PORTB, 0x55);
        let calls = record_listener(&port);
        cpu.write_data(DDRB, 0xf0);
        assert_eq!(calls.calls(), vec![(0x55, 0x55)]);
    }

    #[test]
    fn should_invoke_the_listeners_when_pullup_register_enabled() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        let calls = record_listener(&port);
        cpu.write_data(PORTB, 0x55);
        assert_eq!(calls.calls(), vec![(0x55, 0)]);
    }

    #[test]
    fn should_toggle_the_pin_when_writing_to_the_pin_register() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        let calls = record_listener(&port);
        cpu.write_data(DDRB, 0x0f);
        cpu.write_data(PORTB, 0x55);
        cpu.write_data(PINB, 0x01);
        assert!(calls.calls().contains(&(0x54, 0x55)));
        assert_eq!(cpu.data_byte(PINB), 0x54);
    }

    #[test]
    fn should_only_affect_one_pin_when_writing_to_pin_using_sbi() {
        let asm = asm_program(
            r#"
            _REPLACE DDRD, 0x0a
            _REPLACE PIND, 0x09
            _REPLACE PORTD, 0x0b

            ldi r24, 0x48
            out DDRD, r24
            out PORTD, r24

            sbi PIND, 6

            break
            "#,
        );
        let cpu = Cpu::new(asm.program);
        let port_d = IoPort::new(cpu.clone(), port_d_config());
        let runner = TestProgramRunner::new(cpu.clone());
        let calls = record_listener(&port_d);

        runner.run_instructions(3);
        assert!(calls.calls().contains(&(0x48, 0)));
        assert_eq!(cpu.data_byte(0x2b), 0x48);
        calls.clear();

        runner.run_instructions(1);
        assert!(calls.calls().contains(&(0x08, 0x48)));
        assert_eq!(cpu.data_byte(0x2b), 0x08);
    }

    #[test]
    fn should_update_the_pin_register_on_output_compare_match() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 1 << 1);
        port.timer_override_pin(1, PinOverrideMode::Set);
        assert_eq!(port.pin_state(1), PinState::High);
        assert_eq!(cpu.data_byte(PINB), 1 << 1);
        port.timer_override_pin(1, PinOverrideMode::Clear);
        assert_eq!(port.pin_state(1), PinState::Low);
        assert_eq!(cpu.data_byte(PINB), 0);
    }

    #[test]
    fn should_remove_the_given_listener() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        let calls = Recorder::<(u8, u8)>::new();
        let recorder = calls.clone();
        let id = port.add_listener(Box::new(move |_state, value, old_value| {
            recorder.push((value, old_value));
        }));
        cpu.write_data(DDRB, 0x0f);
        port.remove_listener(id);
        cpu.write_data(PORTB, 0x99);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn should_return_high_when_the_pin_is_output_and_high() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0x1);
        cpu.write_data(PORTB, 0x1);
        assert_eq!(port.pin_state(PB0), PinState::High);
    }

    #[test]
    fn should_return_low_when_the_pin_is_output_and_low() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0x8);
        cpu.write_data(PORTB, 0xf7);
        assert_eq!(port.pin_state(PB3), PinState::Low);
    }

    #[test]
    fn should_return_input_by_default() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        assert_eq!(port.pin_state(PB1), PinState::Input);
    }

    #[test]
    fn should_return_input_pullup_when_pin_is_input_with_pullup() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0);
        cpu.write_data(PORTB, 0x2);
        assert_eq!(port.pin_state(PB1), PinState::InputPullUp);
    }

    #[test]
    fn should_reflect_current_port_state_inside_a_listener() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        let calls = Recorder::<()>::new();
        let recorder = calls.clone();
        let listener_port = port.clone();
        assert_eq!(port.pin_state(PB0), PinState::Input);
        cpu.write_data(DDRB, 0x01);
        port.add_listener(Box::new(move |_state, _value, _old_value| {
            assert_eq!(listener_port.pin_state(PB0), PinState::High);
            recorder.push(());
        }));
        cpu.write_data(PORTB, 0x01);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn should_reflect_current_port_state_inside_a_listener_after_ddr_change() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        let calls = Recorder::<()>::new();
        let recorder = calls.clone();
        let listener_port = port.clone();
        assert_eq!(port.pin_state(PB0), PinState::Input);
        port.add_listener(Box::new(move |_state, _value, _old_value| {
            assert_eq!(listener_port.pin_state(PB0), PinState::Low);
            recorder.push(());
        }));
        cpu.write_data(DDRB, 0x01);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn should_set_the_value_of_the_given_pin() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0);
        port.set_pin(PB4, true);
        assert_eq!(cpu.data_byte(PINB), 0x10);
        port.set_pin(PB4, false);
        assert_eq!(cpu.data_byte(PINB), 0x00);
    }

    #[test]
    fn should_only_update_pin_register_when_pin_is_input() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(DDRB, 0x10);
        cpu.write_data(PORTB, 0);
        port.set_pin(PB4, true);
        assert_eq!(cpu.data_byte(PINB), 0x00);
        cpu.write_data(DDRB, 0);
        assert_eq!(cpu.data_byte(PINB), 0x10);
    }

    #[test]
    fn should_generate_int0_interrupt_on_rising_edge() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        cpu.write_data(EIMSK, 1 << INT0_BIT);
        cpu.write_data(EICRA, (1 << ISC01) | (1 << ISC00));

        assert_eq!(cpu.data_byte(EIFR), 0);
        port.set_pin(PD2, true);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);

        cpu.set_sreg(0x80);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_INT0);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(EIFR), 0);

        port.set_pin(PD2, false);
        assert_eq!(cpu.data_byte(EIFR), 0);
    }

    #[test]
    fn should_generate_int0_interrupt_on_falling_edge() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        cpu.write_data(EIMSK, 1 << INT0_BIT);
        cpu.write_data(EICRA, 1 << ISC01);

        assert_eq!(cpu.data_byte(EIFR), 0);
        port.set_pin(PD2, true);
        assert_eq!(cpu.data_byte(EIFR), 0);
        port.set_pin(PD2, false);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);

        cpu.set_sreg(0x80);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_INT0);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(EIFR), 0);
    }

    #[test]
    fn should_generate_int0_interrupt_on_level_change() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        cpu.write_data(EIMSK, 1 << INT0_BIT);
        cpu.write_data(EICRA, 1 << ISC00);

        assert_eq!(cpu.data_byte(EIFR), 0);
        port.set_pin(PD2, true);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);
        cpu.write_data(EIFR, 1 << INT0_BIT);
        assert_eq!(cpu.data_byte(EIFR), 0);
        port.set_pin(PD2, false);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);
    }

    #[test]
    fn should_keep_a_sticky_int0_interrupt_while_pin_level_is_low() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_d_config());
        cpu.write_data(EIMSK, 1 << INT0_BIT);
        cpu.write_data(EICRA, 0);
        assert_eq!(cpu.data_byte(EIFR), 0);

        port.set_pin(PD2, true);
        assert_eq!(cpu.data_byte(EIFR), 0);

        port.set_pin(PD2, false);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);

        cpu.write_data(EIFR, 1 << INT0_BIT);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);

        cpu.set_sreg(0x80);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_INT0);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(EIFR), 1 << INT0_BIT);

        port.set_pin(PD2, true);
        assert_eq!(cpu.data_byte(EIFR), 0);
    }

    #[test]
    fn should_generate_pin_change_interrupt_when_pb3_goes_high() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(PCICR, 1 << PCIE0);
        cpu.write_data(PCMSK0, 1 << PCINT3);

        port.set_pin(PB3, true);
        assert_eq!(cpu.data_byte(PCIFR), 1 << PCIE0);

        cpu.set_sreg(0x80);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_PCINT0);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(PCIFR), 0);
    }

    #[test]
    fn should_generate_pin_change_interrupt_when_pb3_goes_low() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());

        port.set_pin(PB3, true);
        cpu.write_data(PCICR, 1 << PCIE0);
        cpu.write_data(PCMSK0, 1 << PCINT3);
        assert_eq!(cpu.data_byte(PCIFR), 0);

        port.set_pin(PB3, false);
        assert_eq!(cpu.data_byte(PCIFR), 1 << PCIE0);

        cpu.set_sreg(0x80);
        cpu.tick();
        assert_eq!(cpu.pc(), PC_INT_PCINT0);
        assert_eq!(cpu.cycles(), 2);
        assert_eq!(cpu.data_byte(PCIFR), 0);
    }

    #[test]
    fn should_clear_the_interrupt_flag_when_writing_to_pcifr() {
        let cpu = Cpu::new(vec![0; 1024]);
        let port = IoPort::new(cpu.clone(), port_b_config());
        cpu.write_data(PCICR, 1 << PCIE0);
        cpu.write_data(PCMSK0, 1 << PCINT3);

        port.set_pin(PB3, true);
        assert_eq!(cpu.data_byte(PCIFR), 1 << PCIE0);

        cpu.write_data(PCIFR, 1 << PCIE0);
        assert_eq!(cpu.data_byte(PCIFR), 0);
    }
}
