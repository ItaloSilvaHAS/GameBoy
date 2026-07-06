use crate::cartridge::Cartridge;
use crate::ppu::Ppu;

// =============================================================================
// MAPA DE MEMÓRIA DO GAME BOY (64KB)
// =============================================================================
//  0x0000 – 0x7FFF   ROM do Cartucho (32 KB, banco 0 + banco comutável)
//  0x8000 – 0x9FFF   VRAM (8 KB)
//  0xA000 – 0xBFFF   RAM externa do Cartucho (8 KB, se houver)
//  0xC000 – 0xDFff   WRAM — Work RAM interna (8 KB)
//  0xE000 – 0xFDFF   Echo RAM (espelho de 0xC000–0xDDFF, não usar)
//  0xFE00 – 0xFE9F   OAM — Object Attribute Memory (sprites)
//  0xFEA0 – 0xFEFF   Proibido (retorna 0xFF)
//  0xFF00 – 0xFF7F   Registradores de I/O do hardware
//  0xFF80 – 0xFFFE   HRAM — High RAM (Zero Page rápida)
//  0xFFFF            IE — Interrupt Enable Register

pub struct Bus {
    pub memory:    [u8; 65536],   // RAM geral (WRAM, HRAM, OAM, etc.)
    pub cartridge: Option<Cartridge>,
    pub ppu:       Ppu,

    // Registradores de I/O simples (os que não pertencem ao PPU)
    pub joypad:    u8,   // 0xFF00 — estado dos botões (1 = não pressionado)
    pub sb:        u8,   // 0xFF01 — Serial Transfer Data
    pub sc:        u8,   // 0xFF02 — Serial Transfer Control
    pub div:       u8,   // 0xFF04 — Divider Register (incrementa a 16.384 Hz)
    pub tima:      u8,   // 0xFF05 — Timer Counter
    pub tma:       u8,   // 0xFF06 — Timer Modulo
    pub tac:       u8,   // 0xFF07 — Timer Control
    pub if_reg:    u8,   // 0xFF0F — Interrupt Flag
    pub ie_reg:    u8,   // 0xFFFF — Interrupt Enable
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory:    [0; 65536],
            cartridge: None,
            ppu:       Ppu::new(),
            joypad:    0xFF, // todos os bits em 1 = nenhum botão pressionado
            sb:        0x00,
            sc:        0x00,
            div:       0xAB, // valor inicial do DMG-01 após boot ROM
            tima:      0x00,
            tma:       0x00,
            tac:       0xF8,
            if_reg:    0xE1, // valor inicial do DMG-01
            ie_reg:    0x00,
        }
    }

    pub fn connect_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    // =========================================================================
    // LEITURA
    // =========================================================================
    pub fn read(&self, address: u16) -> u8 {
        match address {
            // ROM do cartucho
            0x0000..=0x7FFF => {
                if let Some(ref cart) = self.cartridge {
                    cart.read(address)
                } else {
                    0xFF
                }
            }

            // VRAM — por enquanto lê da memória geral
            0x8000..=0x9FFF => self.memory[address as usize],

            // RAM externa do cartucho — por enquanto retorna 0xFF
            0xA000..=0xBFFF => 0xFF,

            // WRAM
            0xC000..=0xDFFF => self.memory[address as usize],

            // Echo RAM (espelha WRAM)
            0xE000..=0xFDFF => self.memory[(address - 0x2000) as usize],

            // OAM
            0xFE00..=0xFE9F => self.memory[address as usize],

            // Região proibida
            0xFEA0..=0xFEFF => 0xFF,

            // Registradores de I/O
            0xFF00..=0xFF7F => self.read_io(address),

            // HRAM (High RAM)
            0xFF80..=0xFFFE => self.memory[address as usize],

            // IE — Interrupt Enable
            0xFFFF => self.ie_reg,
        }
    }

    // =========================================================================
    // ESCRITA
    // =========================================================================
    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            // ROM — não escreve (MBC não implementado ainda)
            0x0000..=0x7FFF => { /* TODO: MBC banking */ }

            // VRAM
            0x8000..=0x9FFF => self.memory[address as usize] = value,

            // RAM externa do cartucho
            0xA000..=0xBFFF => { /* TODO: MBC RAM */ }

            // WRAM
            0xC000..=0xDFFF => self.memory[address as usize] = value,

            // Echo RAM
            0xE000..=0xFDFF => self.memory[(address - 0x2000) as usize] = value,

            // OAM
            0xFE00..=0xFE9F => self.memory[address as usize] = value,

            // Região proibida — ignora
            0xFEA0..=0xFEFF => {}

            // Registradores de I/O
            0xFF00..=0xFF7F => self.write_io(address, value),

            // HRAM
            0xFF80..=0xFFFE => self.memory[address as usize] = value,

            // IE
            0xFFFF => self.ie_reg = value,
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
            0xFF0F => self.if_reg,

            // Registradores do PPU
            0xFF40..=0xFF4B => self.ppu.read_register(address),

            // Qualquer outro registrador de I/O não implementado
            _ => 0xFF,
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
            0xFF04 => self.div    = 0, // escrever em DIV sempre reseta para 0
            0xFF05 => self.tima   = value,
            0xFF06 => self.tma    = value,
            0xFF07 => self.tac    = value,
            0xFF0F => self.if_reg = value,

            // Registradores do PPU
            0xFF40..=0xFF4B => self.ppu.write_register(address, value),

            // Ignora escrita em I/O não implementado
            _ => {}
        }
    }
}
