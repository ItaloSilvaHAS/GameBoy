use crate::cartridge::Cartridge;
use crate::ppu::Ppu;

// =============================================================================
// MAPA DE MEMÓRIA DO GAME BOY (64 KB)
// =============================================================================
//  0x0000 – 0x7FFF   ROM do Cartucho  (banco 0 fixo + banco comutável pelo MBC)
//  0x8000 – 0x9FFF   VRAM             (tiles + tilemaps)
//  0xA000 – 0xBFFF   RAM do Cartucho  (salvo / RTC — gerenciado pelo MBC)
//  0xC000 – 0xDFFF   WRAM             (Work RAM interna, 8 KB)
//  0xE000 – 0xFDFF   Echo RAM         (espelho de 0xC000–0xDDFF)
//  0xFE00 – 0xFE9F   OAM              (Object Attribute Memory — sprites)
//  0xFEA0 – 0xFEFF   Proibido
//  0xFF00 – 0xFF7F   Registradores de I/O
//  0xFF80 – 0xFFFE   HRAM             (High RAM)
//  0xFFFF            IE               (Interrupt Enable)

// =============================================================================
// BITS DE INTERRUPÇÃO  (IF = 0xFF0F / IE = 0xFFFF)
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

// =============================================================================
// JOYPAD — registrador 0xFF00
// =============================================================================
// O CPU escreve os bits 5-4 para selecionar qual grupo ler:
//   Bit 4 = 0 → lê D-pad   (Right=bit0, Left=bit1, Up=bit2, Down=bit3)
//   Bit 5 = 0 → lê Botões  (A=bit0,    B=bit1,    Sel=bit2, Start=bit3)
// Em ambos: 0 = pressionado, 1 = solto.

pub struct Joypad {
    pub select:  u8,   // bits 5-4 escritos pelo CPU (seleção de grupo)
    pub buttons: u8,   // A, B, Select, Start  (bits 3-0, 0=pressionado)
    pub dpad:    u8,   // Right, Left, Up, Down (bits 3-0, 0=pressionado)
}

impl Joypad {
    fn new() -> Self {
        // Todos os botões soltos: bits 3-0 = 1
        Self { select: 0x30, buttons: 0x0F, dpad: 0x0F }
    }

    /// Leitura do registrador 0xFF00 pelo CPU.
    fn read(&self) -> u8 {
        let mut val = 0xC0 | self.select; // bits 7-6 sempre 1
        if self.select & 0x10 == 0 { val |= self.dpad    & 0x0F; }
        if self.select & 0x20 == 0 { val |= self.buttons & 0x0F; }
        // Se nenhum grupo selecionado (ambos bits altos), retorna 0xFF
        if self.select & 0x30 == 0x30 { val |= 0x0F; }
        val
    }
}

pub struct Bus {
    pub memory:    [u8; 65536],
    pub cartridge: Option<Cartridge>,
    pub ppu:       Ppu,

    // I/O
    pub joy:       Joypad, // 0xFF00 — joypad com seleção de grupo
    pub sb:        u8,     // 0xFF01 — Serial data
    pub sc:        u8,     // 0xFF02 — Serial control
    pub if_reg:    u8,     // 0xFF0F — Interrupt Flag
    pub ie_reg:    u8,     // 0xFFFF — Interrupt Enable

    // Timer
    div_cycles:    u32,
    pub div:       u8,     // 0xFF04 — Divider (reset-on-write)
    pub tima:      u8,     // 0xFF05 — Timer Counter
    pub tma:       u8,     // 0xFF06 — Timer Modulo
    pub tac:       u8,     // 0xFF07 — Timer Control
    tima_cycles:   u32,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory:       [0u8; 65536],
            cartridge:    None,
            ppu:          Ppu::new(),
            joy:          Joypad::new(),
            sb:           0x00,
            sc:           0x7E,
            if_reg:       0xE1,
            ie_reg:       0x00,
            div_cycles:   0,
            div:          0xAB,
            tima:         0x00,
            tma:          0x00,
            tac:          0xF8,
            tima_cycles:  0,
        }
    }

    pub fn connect_cartridge(&mut self, cart: Cartridge) {
        self.cartridge = Some(cart);
    }

    // =========================================================================
    // TICK — avança todo o hardware pelo número de ciclos de uma instrução.
    // =========================================================================
    pub fn tick(&mut self, cycles: u32) {
        self.tick_timer(cycles);

        // Borrow split: memory e ppu são campos distintos — o compilador aceita.
        let vram = &self.memory[0x8000..0xA000]; // 8 KB de tiles + tilemaps
        let oam  = &self.memory[0xFE00..0xFEA0]; // 160 bytes de OAM (40 sprites)
        let events = self.ppu.step(cycles, vram, oam);

        if events.vblank                                        { self.if_reg |= INT_VBLANK; }
        if events.stat_hblank | events.stat_vblank
           | events.stat_oam  | events.stat_lyc                { self.if_reg |= INT_STAT;   }
    }

    // =========================================================================
    // TIMER
    // =========================================================================
    fn tick_timer(&mut self, cycles: u32) {
        // DIV: incrementa a cada 256 ciclos (16.384 Hz)
        self.div_cycles += cycles;
        if self.div_cycles >= 256 {
            self.div_cycles -= 256;
            self.div = self.div.wrapping_add(1);
        }

        // TIMA só corre se TAC bit 2 estiver setado
        if self.tac & 0x04 == 0 { return; }

        // Frequência do TIMA: bits 1-0 do TAC
        //  00 →  4.096 Hz = 1 tick/1.024 ciclos
        //  01 → 262.144 Hz = 1 tick/16 ciclos
        //  10 →  65.536 Hz = 1 tick/64 ciclos
        //  11 →  16.384 Hz = 1 tick/256 ciclos
        let threshold: u32 = match self.tac & 0x03 {
            0 => 1024,
            1 =>   16,
            2 =>   64,
            _ =>  256,
        };

        self.tima_cycles += cycles;
        while self.tima_cycles >= threshold {
            self.tima_cycles -= threshold;
            let (val, overflow) = self.tima.overflowing_add(1);
            if overflow {
                self.tima   = self.tma;
                self.if_reg |= INT_TIMER;
            } else {
                self.tima = val;
            }
        }
    }

    // =========================================================================
    // LEITURA
    // =========================================================================
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if let Some(ref c) = self.cartridge { c.read(addr) } else { 0xFF }
            }
            0x8000..=0x9FFF => self.memory[addr as usize],
            0xA000..=0xBFFF => {
                if let Some(ref c) = self.cartridge { c.read(addr) } else { 0xFF }
            }
            0xC000..=0xDFFF => self.memory[addr as usize],
            0xE000..=0xFDFF => self.memory[(addr - 0x2000) as usize],
            0xFE00..=0xFE9F => self.memory[addr as usize],
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00..=0xFF7F => self.read_io(addr),
            0xFF80..=0xFFFE => self.memory[addr as usize],
            0xFFFF          => self.ie_reg,
        }
    }

    // =========================================================================
    // ESCRITA
    // =========================================================================
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => {
                // Escritas na área da ROM são interceptadas pelo MBC
                if let Some(ref mut c) = self.cartridge { c.write(addr, value); }
            }
            0x8000..=0x9FFF => self.memory[addr as usize] = value,
            0xA000..=0xBFFF => {
                // RAM externa / RTC do cartucho
                if let Some(ref mut c) = self.cartridge { c.write(addr, value); }
            }
            0xC000..=0xDFFF => self.memory[addr as usize] = value,
            0xE000..=0xFDFF => self.memory[(addr - 0x2000) as usize] = value,
            0xFE00..=0xFE9F => self.memory[addr as usize] = value,
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.write_io(addr, value),
            0xFF80..=0xFFFE => self.memory[addr as usize] = value,
            0xFFFF          => self.ie_reg = value,
        }
    }

    // =========================================================================
    // I/O — leitura
    // =========================================================================
    fn read_io(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joy.read(),
            0xFF01 => self.sb,
            0xFF02 => self.sc,
            0xFF03 => 0xFF,
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac | 0xF8,           // bits 7-3 sempre 1
            0xFF08..=0xFF0E => 0xFF,
            0xFF0F => self.if_reg | 0xE0,         // bits 7-5 sempre 1
            0xFF40..=0xFF4B => self.ppu.read_register(addr),
            _ => 0xFF,
        }
    }

    // =========================================================================
    // I/O — escrita
    // =========================================================================
    fn write_io(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF00 => self.joy.select = value & 0x30, // só bits 5-4 são graváveis
            0xFF01 => self.sb     = value,
            0xFF02 => self.sc     = value,
            0xFF04 => { self.div = 0; self.div_cycles = 0; } // escrita reseta DIV
            0xFF05 => self.tima   = value,
            0xFF06 => self.tma    = value,
            0xFF07 => self.tac    = value & 0x07,
            0xFF0F => self.if_reg = value,
            0xFF40..=0xFF4B => self.ppu.write_register(addr, value),
            _ => {}
        }
    }
}
