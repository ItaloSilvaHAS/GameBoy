// =============================================================================
// PPU — Picture Processing Unit
// =============================================================================
// Timing do LCD do DMG-01:
//   1 scanline  = 456 ciclos de clock
//   144 linhas visíveis + 10 linhas de VBlank = 154 linhas totais
//   1 frame completo = 154 × 456 = 70.224 ciclos  (~59,7 fps)
//
// Modos por scanline:
//   Mode 2 (OAM Search):    ciclos   0 –  79  (80 ciclos)
//   Mode 3 (LCD Transfer):  ciclos  80 – 251  (172 ciclos)
//   Mode 0 (HBlank):        ciclos 252 – 455  (204 ciclos)
//   Mode 1 (VBlank):        linhas 144 – 153  (456 ciclos/linha)

// Eventos que o PPU pode emitir num único step, lidos pelo Bus para disparar
// interrupções nos registradores IF.
pub struct PpuEvents {
    pub vblank:       bool, // entrou em VBlank (linha 144) → IF bit 0
    pub stat_hblank:  bool, // entrou em HBlank com STAT bit 3 habilitado → IF bit 1
    pub stat_vblank:  bool, // entrou em VBlank com STAT bit 4 habilitado → IF bit 1
    pub stat_oam:     bool, // entrou em OAM    com STAT bit 5 habilitado → IF bit 1
    pub stat_lyc:     bool, // LY == LYC        com STAT bit 6 habilitado → IF bit 1
}

impl PpuEvents {
    fn none() -> Self {
        Self { vblank: false, stat_hblank: false, stat_vblank: false,
               stat_oam: false, stat_lyc: false }
    }
}

pub struct Ppu {
    pub cycles:    u32,  // ciclos dentro da scanline atual
    pub ly:        u8,   // linha atual do LCD (0–153)
    pub lyc:       u8,   // LY Compare
    pub lcdc:      u8,   // LCD Control
    pub stat:      u8,   // LCD Status
    pub scy:       u8,   // Scroll Y
    pub scx:       u8,   // Scroll X
    pub wy:        u8,   // Window Y
    pub wx:        u8,   // Window X
    pub bgp:       u8,   // Background Palette
    pub obp0:      u8,   // Object Palette 0
    pub obp1:      u8,   // Object Palette 1
    prev_mode:     u8,   // modo do frame anterior (para detectar transições)
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycles:    0,
            ly:        0,
            lyc:       0,
            lcdc:      0x91,
            stat:      0x85,
            scy:       0,
            scx:       0,
            wy:        0,
            wx:        0,
            bgp:       0xFC,
            obp0:      0xFF,
            obp1:      0xFF,
            prev_mode: 1,
        }
    }

    /// Avança o PPU e retorna os eventos ocorridos neste tick.
    pub fn step(&mut self, cycles: u32) -> PpuEvents {
        let mut events = PpuEvents::none();

        // LCD desligado: congela tudo
        if self.lcdc & 0x80 == 0 {
            self.ly        = 0;
            self.cycles    = 0;
            self.stat      = self.stat & !0x03;
            self.prev_mode = 0;
            return events;
        }

        self.cycles += cycles;

        // Avança a scanline a cada 456 ciclos
        if self.cycles >= 456 {
            self.cycles -= 456;
            let old_ly = self.ly;
            self.ly = (self.ly + 1) % 154;

            // Transição para VBlank (linha 143 → 144)
            if old_ly == 143 && self.ly == 144 {
                events.vblank = true;
                if self.stat & 0x10 != 0 { events.stat_vblank = true; }
            }
        }

        // Calcula o modo atual
        let mode = if self.ly >= 144 {
            1 // VBlank
        } else {
            match self.cycles {
                0..=79   => 2, // OAM Search
                80..=251 => 3, // LCD Transfer
                _        => 0, // HBlank
            }
        };

        // Detecta transições de modo para STAT interrupts
        if mode != self.prev_mode {
            match mode {
                0 => { if self.stat & 0x08 != 0 { events.stat_hblank = true; } }
                2 => { if self.stat & 0x20 != 0 { events.stat_oam    = true; } }
                _ => {}
            }
            self.prev_mode = mode;
        }

        // Atualiza modo no STAT (bits 1-0)
        self.stat = (self.stat & !0x03) | mode;

        // Bit 2 de STAT: coincidência LY == LYC
        let lyc_match = self.ly == self.lyc;
        if lyc_match {
            if self.stat & 0x04 == 0 {          // borda de subida
                self.stat |= 0x04;
                if self.stat & 0x40 != 0 { events.stat_lyc = true; }
            }
        } else {
            self.stat &= !0x04;
        }

        events
    }

    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _      => 0xFF,
        }
    }

    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF40 => self.lcdc = value,
            0xFF41 => self.stat = (self.stat & 0x07) | (value & 0xF8),
            0xFF42 => self.scy  = value,
            0xFF43 => self.scx  = value,
            0xFF44 => { self.ly = 0; } // somente leitura — escrita reseta
            0xFF45 => self.lyc  = value,
            0xFF47 => self.bgp  = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy   = value,
            0xFF4B => self.wx   = value,
            _      => {}
        }
    }
}
