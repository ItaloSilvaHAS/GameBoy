// =============================================================================
// CARTRIDGE — MBC3 (Memory Bank Controller 3)
// =============================================================================
//
// O Pokémon Red/Blue é um cartucho MBC3+RAM+BATTERY (tipo 0x13 no header).
// A ROM tem 1 MB (64 bancos × 16 KB). A RAM tem 32 KB (4 bancos × 8 KB),
// usada para salvar o progresso do jogo.
//
// MAPA DE ENDEREÇOS
// ─────────────────────────────────────────────────────────────────────────────
//  0x0000–0x3FFF  ROM banco 0      (fixo, nunca comuta)
//  0x4000–0x7FFF  ROM banco N      (comutável, padrão = banco 1)
//  0xA000–0xBFFF  RAM externa      (banco comutável, quando habilitada)
//                 ou registrador RTC (quando selecionado)
//
// REGISTRADORES MBC3 (escritas na área da ROM pelo jogo)
// ─────────────────────────────────────────────────────────────────────────────
//  Esc 0x0000–0x1FFF  RAM Enable   — 0x0A habilita RAM/RTC; qualquer outro valor desabilita
//  Esc 0x2000–0x3FFF  ROM Bank     — bits 6..0, valores 0x01–0x7F (0 → tratado como 1)
//  Esc 0x4000–0x5FFF  RAM Bank / RTC Select
//                       0x00–0x03  → seleciona banco de RAM 0–3
//                       0x08–0x0C  → seleciona registrador RTC (S/M/H/DL/DH)
//  Esc 0x6000–0x7FFF  Latch RTC   — sequência 0x00 → 0x01 congela o RTC

use std::fs;
use std::path::Path;
use std::io;

// ─────────────────────────────────────────────────────────────────────────────
// Tipo de MBC detectado a partir do byte 0x0147 do header
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, PartialEq)]
enum MbcType {
    NoMbc,   // ROM only
    Mbc1,    // MBC1 — não implementado, stub
    Mbc3,    // MBC3 — implementação completa aqui
}

// ─────────────────────────────────────────────────────────────────────────────
// Registradores RTC do MBC3
// O jogo usa o RTC para rastrear o tempo real (horas jogadas, eventos diários).
// Nossa implementação mantém os registradores em memória mas não avança o tempo
// — suficiente para o jogo não travar ao ler/escrever o RTC.
// ─────────────────────────────────────────────────────────────────────────────
struct Rtc {
    seconds:  u8,   // 0x08 — S  (0–59)
    minutes:  u8,   // 0x09 — M  (0–59)
    hours:    u8,   // 0x0A — H  (0–23)
    days_lo:  u8,   // 0x0B — DL (bit 7..0 do contador de dias)
    days_hi:  u8,   // 0x0C — DH (bit 0 = day bit 8, bit 6 = halt, bit 7 = carry)
    // registradores "latched" — congelados quando o jogo faz o latch
    lat_s:    u8,
    lat_m:    u8,
    lat_h:    u8,
    lat_dl:   u8,
    lat_dh:   u8,
    latch_state: u8, // estado da sequência de latch (espera 0x00 → 0x01)
}

impl Rtc {
    fn new() -> Self {
        Self {
            seconds: 0, minutes: 0, hours: 0, days_lo: 0, days_hi: 0,
            lat_s: 0, lat_m: 0, lat_h: 0, lat_dl: 0, lat_dh: 0,
            latch_state: 0xFF,
        }
    }

    /// Lê o registrador latched selecionado (reg = 0x08..0x0C)
    fn read(&self, reg: u8) -> u8 {
        match reg {
            0x08 => self.lat_s,
            0x09 => self.lat_m,
            0x0A => self.lat_h,
            0x0B => self.lat_dl,
            0x0C => self.lat_dh,
            _    => 0xFF,
        }
    }

    /// Escreve no registrador ativo (reg = 0x08..0x0C)
    fn write(&mut self, reg: u8, value: u8) {
        match reg {
            0x08 => self.seconds = value & 0x3F,
            0x09 => self.minutes = value & 0x3F,
            0x0A => self.hours   = value & 0x1F,
            0x0B => self.days_lo = value,
            0x0C => self.days_hi = value & 0xC1,
            _    => {}
        }
    }

    /// Processa escrita no registrador de latch (0x6000–0x7FFF).
    /// Sequência 0x00 → 0x01 congela os valores atuais nos registradores latched.
    fn latch(&mut self, value: u8) {
        if self.latch_state == 0x00 && value == 0x01 {
            self.lat_s  = self.seconds;
            self.lat_m  = self.minutes;
            self.lat_h  = self.hours;
            self.lat_dl = self.days_lo;
            self.lat_dh = self.days_hi;
        }
        self.latch_state = value;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Struct principal
// ─────────────────────────────────────────────────────────────────────────────
pub struct Cartridge {
    pub rom:          Vec<u8>,
    pub ram:          Vec<u8>,         // RAM externa (salvo de jogo)
    mbc:              MbcType,
    rom_bank:         usize,           // banco de ROM selecionado (1–127)
    ram_bank:         usize,           // banco de RAM selecionado (0–3)
    ram_enabled:      bool,            // RAM/RTC habilitados
    rtc_selected:     bool,            // true quando um reg RTC está selecionado
    rtc_reg:          u8,              // reg RTC ativo (0x08..0x0C)
    rtc:              Rtc,
    num_rom_banks:    usize,
    num_ram_banks:    usize,
}

impl Cartridge {
    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let rom = fs::read(path)?;

        if rom.len() < 0x150 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "ROM menor que 0x150 bytes"));
        }

        // ── Lê metadados do header ────────────────────────────────────────────
        let cart_type_byte = rom[0x0147];
        let rom_size_byte  = rom[0x0148];
        let ram_size_byte  = rom[0x0149];

        let mbc = match cart_type_byte {
            0x00 | 0x08 | 0x09                   => MbcType::NoMbc,
            0x01 | 0x02 | 0x03                   => MbcType::Mbc1,
            0x11 | 0x12 | 0x13 | 0x0F | 0x10     => MbcType::Mbc3,
            other => {
                println!("  [AVISO] Tipo de cartucho desconhecido: {:#04X} — tratando como NoMBC", other);
                MbcType::NoMbc
            }
        };

        // Número de bancos de ROM (cada banco = 16 KB)
        let num_rom_banks = match rom_size_byte {
            0x00 => 2,   //  32 KB —  2 bancos
            0x01 => 4,   //  64 KB —  4 bancos
            0x02 => 8,   // 128 KB —  8 bancos
            0x03 => 16,  // 256 KB — 16 bancos
            0x04 => 32,  // 512 KB — 32 bancos
            0x05 => 64,  //   1 MB — 64 bancos
            0x06 => 128, //   2 MB — 128 bancos
            0x07 => 256, //   4 MB — 256 bancos
            0x08 => 512, //   8 MB — 512 bancos
            _    => 2,
        };

        // Número de bancos de RAM (cada banco = 8 KB)
        let num_ram_banks = match ram_size_byte {
            0x00 => 0,
            0x01 => 1, //  2 KB (consideramos 1 banco parcial)
            0x02 => 1, //  8 KB — 1 banco
            0x03 => 4, // 32 KB — 4 bancos
            0x04 => 16,// 128 KB — 16 bancos
            0x05 => 8, // 64 KB — 8 bancos
            _    => 0,
        };

        let ram_size = if num_ram_banks == 0 { 0 } else { num_ram_banks * 8192 };
        let ram = vec![0u8; ram_size];

        // ── Imprime diagnóstico ───────────────────────────────────────────────
        let title: String = rom[0x0134..0x0143]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();

        println!("-> ROM Carregada: \"{}\" ({} bytes)", title, rom.len());
        println!("   Tipo: {:#04X} ({:?}) | ROM: {} bancos × 16 KB | RAM: {} bancos × 8 KB",
            cart_type_byte, mbc, num_rom_banks, num_ram_banks);

        Ok(Self {
            rom,
            ram,
            mbc,
            rom_bank:      1,
            ram_bank:      0,
            ram_enabled:   false,
            rtc_selected:  false,
            rtc_reg:       0x08,
            rtc:           Rtc::new(),
            num_rom_banks,
            num_ram_banks,
        })
    }

    // =========================================================================
    // LEITURA
    // =========================================================================

    /// Lê da ROM (0x0000–0x7FFF) ou da RAM/RTC (0xA000–0xBFFF)
    pub fn read(&self, address: u16) -> u8 {
        match address {
            // Banco 0 — sempre fixo
            0x0000..=0x3FFF => {
                let idx = address as usize;
                if idx < self.rom.len() { self.rom[idx] } else { 0xFF }
            }

            // Banco N — comutável
            0x4000..=0x7FFF => {
                let bank = match self.mbc {
                    MbcType::NoMbc | MbcType::Mbc1 => 1,
                    MbcType::Mbc3                  => self.rom_bank,
                };
                let offset = (address as usize) - 0x4000;
                let idx    = bank * 0x4000 + offset;
                if idx < self.rom.len() { self.rom[idx] } else { 0xFF }
            }

            // RAM externa / RTC
            0xA000..=0xBFFF => {
                if !self.ram_enabled { return 0xFF; }
                if self.rtc_selected { return self.rtc.read(self.rtc_reg); }
                if self.num_ram_banks == 0 { return 0xFF; }
                let offset = (address as usize) - 0xA000;
                let idx    = self.ram_bank * 0x2000 + offset;
                if idx < self.ram.len() { self.ram[idx] } else { 0xFF }
            }

            _ => 0xFF,
        }
    }

    // =========================================================================
    // ESCRITA — intercepta escritas na área da ROM para controlar o MBC
    // =========================================================================
    pub fn write(&mut self, address: u16, value: u8) {
        match self.mbc {
            MbcType::NoMbc => {}

            MbcType::Mbc1 => {
                // MBC1 simplificado — apenas troca banco de ROM (bits 4..0)
                if (0x2000..=0x3FFF).contains(&address) {
                    let bank = (value & 0x1F) as usize;
                    self.rom_bank = if bank == 0 { 1 } else { bank };
                }
            }

            MbcType::Mbc3 => self.mbc3_write(address, value),
        }
    }

    // =========================================================================
    // MBC3 — lógica completa de registradores
    // =========================================================================
    fn mbc3_write(&mut self, address: u16, value: u8) {
        match address {
            // ── RAM / RTC Enable ──────────────────────────────────────────────
            // Qualquer escrita com nibble baixo = 0xA habilita; resto desabilita.
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }

            // ── ROM Bank Number ───────────────────────────────────────────────
            // Bits 6..0 selecionam o banco (1–127). Valor 0 → tratado como 1.
            // O banco é mascarado pelo número real de bancos para não sair da ROM.
            0x2000..=0x3FFF => {
                // Bits 6..0 do valor escrito selecionam o banco.
                // Valor 0 → redireciona para banco 1 (hardware real faz isso).
                // Após mascarar pelo número de bancos disponíveis, garante que o
                // resultado nunca seja 0 (banco 0 só é acessível em 0x0000–0x3FFF).
                let requested = (value & 0x7F) as usize;
                let bank = if requested == 0 { 1 } else { requested };
                let bank = bank % self.num_rom_banks;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }

            // ── RAM Bank / RTC Select ─────────────────────────────────────────
            // 0x00–0x03 → banco de RAM
            // 0x08–0x0C → registrador RTC
            0x4000..=0x5FFF => {
                match value {
                    0x00..=0x03 => {
                        self.rtc_selected = false;
                        self.ram_bank     = value as usize;
                    }
                    0x08..=0x0C => {
                        self.rtc_selected = true;
                        self.rtc_reg      = value;
                    }
                    _ => {} // valores não mapeados são ignorados
                }
            }

            // ── Latch Clock Data ──────────────────────────────────────────────
            0x6000..=0x7FFF => {
                self.rtc.latch(value);
            }

            // ── Escrita na RAM externa ────────────────────────────────────────
            0xA000..=0xBFFF => {
                if !self.ram_enabled { return; }
                if self.rtc_selected {
                    self.rtc.write(self.rtc_reg, value);
                    return;
                }
                if self.num_ram_banks == 0 { return; }
                let offset = (address as usize) - 0xA000;
                let idx    = self.ram_bank * 0x2000 + offset;
                if idx < self.ram.len() {
                    self.ram[idx] = value;
                }
            }

            _ => {}
        }
    }
}
