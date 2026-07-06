// =============================================================================
// PPU — Picture Processing Unit (stub de hardware)
// =============================================================================
// O PPU real do Game Boy é responsável por desenhar pixels na tela.
// Por enquanto, implementamos apenas o comportamento de TIMING correto,
// ou seja: os registradores de status (LY, STAT, LCDC) respondem como se
// o hardware real estivesse rodando — sem renderizar nada ainda.
//
// Timing do LCD do DMG-01:
//   1 scanline  = 456 ciclos de clock
//   144 linhas visíveis + 10 linhas de VBlank = 154 linhas totais
//   1 frame completo = 154 × 456 = 70.224 ciclos  (~59,7 fps)
//
// Dentro de cada scanline, o PPU passa por 3 modos:
//   Mode 2 (OAM Search):   ciclos   0 –  79  (80 ciclos)
//   Mode 3 (LCD Transfer):  ciclos  80 – 251  (172 ciclos)
//   Mode 0 (HBlank):        ciclos 252 – 455  (204 ciclos)
//
// Durante as linhas 144–153 o PPU está em:
//   Mode 1 (VBlank):        456 ciclos por linha

pub struct Ppu {
    pub cycles: u32,  // ciclos acumulados dentro do frame atual
    pub ly:     u8,   // linha atual do LCD (0–153)
    pub lyc:    u8,   // LY Compare — usado para gerar interrupção quando LY == LYC
    pub lcdc:   u8,   // LCD Control register
    pub stat:   u8,   // LCD Status register
    pub scy:    u8,   // Scroll Y
    pub scx:    u8,   // Scroll X
    pub wy:     u8,   // Window Y
    pub wx:     u8,   // Window X
    pub bgp:    u8,   // Background Palette Data
    pub obp0:   u8,   // Object Palette 0
    pub obp1:   u8,   // Object Palette 1
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycles: 0,
            ly:     0,
            lyc:    0,
            lcdc:   0x91, // LCD ligado, BG ligado — valor padrão do boot
            stat:   0x85,
            scy:    0,
            scx:    0,
            wy:     0,
            wx:     0,
            bgp:    0xFC, // paleta padrão do Game Boy (branco/cinza/preto)
            obp0:   0xFF,
            obp1:   0xFF,
        }
    }

    /// Avança o PPU pelo número de ciclos consumidos pela última instrução da CPU.
    /// Atualiza LY e o modo do STAT automaticamente.
    pub fn step(&mut self, cycles: u32) {
        // Se o LCD estiver desligado (bit 7 do LCDC = 0), congela tudo
        if self.lcdc & 0x80 == 0 {
            self.ly     = 0;
            self.cycles = 0;
            self.stat   = self.stat & !0x03; // modo 0
            return;
        }

        self.cycles += cycles;

        // Cada scanline dura 456 ciclos
        if self.cycles >= 456 {
            self.cycles -= 456;
            self.ly = (self.ly + 1) % 154; // wrap de 153 para 0
        }

        // Atualiza o modo no STAT (bits 1-0) e o bit de coincidência LY==LYC (bit 2)
        let mode = if self.ly >= 144 {
            1 // VBlank
        } else {
            match self.cycles {
                0..=79   => 2, // OAM Search
                80..=251 => 3, // LCD Transfer
                _        => 0, // HBlank
            }
        };

        // Seta bits 1-0 com o modo atual; preserva os outros bits
        self.stat = (self.stat & !0x03) | mode;

        // Bit 2: coincidência LY == LYC
        if self.ly == self.lyc {
            self.stat |= 0x04;
        } else {
            self.stat &= !0x04;
        }
    }

    /// Lê um registrador de I/O do PPU pelo endereço do barramento.
    pub fn read_register(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80, // bit 7 sempre 1 (não usado, mas leitura retorna 1)
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => 0xFF, // registrador não implementado retorna 0xFF (padrão do hardware)
        }
    }

    /// Escreve em um registrador de I/O do PPU.
    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF40 => self.lcdc = value,
            0xFF41 => {
                // Bits 0-2 são somente leitura; só preservamos os bits 3-7 da escrita
                self.stat = (self.stat & 0x07) | (value & 0xF8);
            }
            0xFF42 => self.scy  = value,
            0xFF43 => self.scx  = value,
            0xFF44 => { /* LY é somente leitura — escrita reseta para 0 no hardware */ self.ly = 0; }
            0xFF45 => self.lyc  = value,
            0xFF47 => self.bgp  = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy   = value,
            0xFF4B => self.wx   = value,
            _ => { /* ignora escrita em registrador não implementado */ }
        }
    }
}
