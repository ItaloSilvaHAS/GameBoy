use crate::cartridge::Cartridge;
use crate::ppu::Ppu;

// =============================================================================
// MAPA DE MEMÓRIA DO GAME BOY (64KB)
// =============================================================================
//  0x0000 – 0x7FFF   ROM do Cartucho (32 KB, banco 0 + banco comutável)
//  0x8000 – 0x9FFF   VRAM (8 KB)
//  0xA000 – 0xBFFF   RAM externa do Cartucho (8 KB, se houver)
//  0xC000 – 0xDFFF   WRAM — Work RAM interna (8 KB)
//  0xE000 – 0xFDFF   Echo RAM (espelho de 0xC000–0xDDFF)
//  0xFE00 – 0xFE9F   OAM — Object Attribute Memory (sprites)
//  0xFEA0 – 0xFEFF   Proibido
//  0xFF00 – 0xFF7F   Registradores de I/O
//  0xFF80 – 0xFFFE   HRAM — High RAM
//  0xFFFF            IE — Interrupt Enable

// =============================================================================
// BITS DE INTERRUPÇÃO (IF / IE — 0xFF0F / 0xFFFF)
// =============================================================================
//  Bit 0 — VBlank   → handler em 0x0040
//  Bit 1 — LCD STAT → handler em 0x0048
//  Bit 2 — Timer    → handler em 0x0050
//  Bit 3 — Serial   → handler em 0x0058
//  Bit 4 — Joypad   → handler em 0x0060
pub const INT_VBLANK: u8 = 1 << 0;
pub const INT_STAT:   u8 = 1 << 1;
pub const INT_TIMER:  u8 = 1 << 2;
#[allow(dead_code)]
pub const INT_SERIAL: u8 = 1 << 3;
#[allow(dead_code)]
pub const INT_JOYPAD: u8 = 1 << 4;

pub struct Bus {
    pub memory:    [u8; 65536],
    pub cartridge: Option<Cartridge>,
    pub ppu:       Ppu,

    // I/O
    pub joypad:    u8,   // 0xFF00
    pub sb:        u8,   // 0xFF01
    pub sc:        u8,   // 0xFF02
    pub if_reg:    u8,   // 0xFF0F — Interrupt Flag
    pub ie_reg:    u8,   // 0xFFFF — Interrupt Enable

    // Timer
    div_cycles:    u32,  // acumulador interno para DIV (incrementa a cada 256 ciclos)
    pub div:       u8,   // 0xFF04 — Divider Register (visível ao jogo)
    pub tima:      u8,   // 0xFF05 — Timer Counter
    pub tma:       u8,   // 0xFF06 — Timer Modulo
    pub tac:       u8,   // 0xFF07 — Timer Control
    tima_cycles:   u32,  // acumulador interno para TIMA
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory:       [0; 65536],
            cartridge:    None,
            ppu:          Ppu::new(),
            joypad:       0xFF,
            sb:           0x00,
            sc:           0x7E,
            if_reg:       0xE1, // valor do DMG-01 após boot ROM
            ie_reg:       0x00,
            div_cycles:   0,
            div:          0xAB, // valor do DMG-01 após boot ROM
            tima:         0x00,
            tma:          0x00,
            tac:          0xF8,
            tima_cycles:  0,
        }
    }

    pub fn connect_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    // =========================================================================
    // TICK — avança todo o hardware pelo número de ciclos de uma instrução.
    // Chamado pelo main loop após cada cpu.step().
    // =========================================================================
    pub fn tick(&mut self, cycles: u32) {
        self.tick_timer(cycles);
        let events = self.ppu.step(cycles);

        // Dispara interrupções baseado nos eventos do PPU
        if events.vblank {
            self.if_reg |= INT_VBLANK;
        }
        if events.stat_hblank || events.stat_vblank || events.stat_oam || events.stat_lyc {
            self.if_reg |= INT_STAT;
        }
    }

    // =========================================================================
    // TIMER
    // =========================================================================
    fn tick_timer(&mut self, cycles: u32) {
        // DIV incrementa a cada 256 ciclos (16.384 Hz)
        self.div_cycles += cycles;
        if self.div_cycles >= 256 {
            self.div_cycles -= 256;
            self.div = self.div.wrapping_add(1);
        }

        // TIMA só funciona se o bit 2 do TAC estiver setado
        if self.tac & 0x04 == 0 { return; }

        // Frequência do TIMA determinada pelos bits 1-0 do TAC:
        //  00 → 4.096 Hz   = 1 tick a cada 1.024 ciclos
        //  01 → 262.144 Hz = 1 tick a cada    16 ciclos
        //  10 → 65.536 Hz  = 1 tick a cada    64 ciclos
        //  11 → 16.384 Hz  = 1 tick a cada   256 ciclos
        let threshold = match self.tac & 0x03 {
            0 => 1024,
            1 =>   16,
            2 =>   64,
            _ =>  256,
        };

        self.tima_cycles += cycles;
        while self.tima_cycles >= threshold {
            self.tima_cycles -= threshold;
            let (new_tima, overflow) = self.tima.overflowing_add(1);
            if overflow {
                // TIMA transbordou: reseta para TMA e pede interrupção
                self.tima = self.tma;
                self.if_reg |= INT_TIMER;
            } else {
                self.tima = new_tima;
            }
        }
    }

    // =========================================================================
    // LEITURA
    // =========================================================================
    pub fn read(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => {
                if let Some(ref cart) = self.cartridge { cart.read(address) } else { 0xFF }
            }
            0x8000..=0x9FFF => self.memory[address as usize],
            0xA000..=0xBFFF => {
                if let Some(ref cart) = self.cartridge { cart.read(address) } else { 0xFF }
            }
            0xC000..=0xDFFF => self.memory[address as usize],
            0xE000..=0xFDFF => self.memory[(address - 0x2000) as usize],
            0xFE00..=0xFE9F => self.memory[address as usize],
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00..=0xFF7F => self.read_io(address),
            0xFF80..=0xFFFE => self.memory[address as usize],
            0xFFFF          => self.ie_reg,
        }
    }

    // =========================================================================
    // ESCRITA
    // =========================================================================
    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => {
                // Escritas na área da ROM são interceptadas pelo MBC (troca de bancos)
                if let Some(ref mut cart) = self.cartridge {
                    cart.write(address, value);
                }
            }
            0x8000..=0x9FFF => self.memory[address as usize] = value,
            0xA000..=0xBFFF => {
                // RAM externa do cartucho (ou RTC no MBC3)
                if let Some(ref mut cart) = self.cartridge {
                    cart.write(address, value);
                }
            }
            0xC000..=0xDFFF => self.memory[address as usize] = value,
            0xE000..=0xFDFF => self.memory[(address - 0x2000) as usize] = value,
            0xFE00..=0xFE9F => self.memory[address as usize] = value,
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(address, value),
            0xFF80..=0xFFFE => self.memory[address as usize] = value,
            0xFFFF          => self.ie_reg = value,
        }
    }

    // =========================================================================
    // I/O — leitura
    // =========================================================================
    fn read_io(&self, address: u16) -> u8 {
        match address {
            0xFF00 => self.joypad,
            0xFF01 => self.sb,
            0xFF02 => self.sc,
            0xFF03 => 0xFF,
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac,
            0xFF08..=0xFF0E => 0xFF,
            0xFF0F => self.if_reg | 0xE0, // bits 7-5 sempre 1 na leitura
            0xFF40..=0xFF4B => self.ppu.read_register(address),
            _      => 0xFF,
        }
    }

    // =========================================================================
    // I/O — escrita
    // =========================================================================
    fn write_io(&mut self, address: u16, value: u8) {
        match address {
            0xFF00 => self.joypad = value,
            0xFF01 => self.sb     = value,
            0xFF02 => self.sc     = value,
            0xFF04 => { self.div = 0; self.div_cycles = 0; } // qualquer escrita reseta
            0xFF05 => self.tima   = value,
            0xFF06 => self.tma    = value,
            0xFF07 => self.tac    = value & 0x07,
            0xFF0F => self.if_reg = value,
            0xFF40..=0xFF4B => self.ppu.write_register(address, value),
            _      => {}
        }
    }
}
