// =============================================================================
// PPU — Picture Processing Unit  (scanline renderer completo)
// =============================================================================
//
// TIMING DO LCD (DMG-01)
//   1 scanline  = 456 ciclos
//   Linhas 0–143  → visíveis (renderizadas)
//   Linhas 144–153 → VBlank
//   1 frame = 154 × 456 = 70.224 ciclos  (~59,7 fps)
//
// MODOS POR SCANLINE VISÍVEL
//   Mode 2 — OAM Search     ciclos   0–79    (80 ciclos)
//   Mode 3 — LCD Transfer   ciclos  80–251   (172 ciclos)  ← renderiza aqui
//   Mode 0 — HBlank         ciclos 252–455   (204 ciclos)
//
// LCDC (0xFF40) — bits de controle
//   Bit 7: LCD Enable
//   Bit 6: Window tilemap  (0=0x9800, 1=0x9C00)
//   Bit 5: Window Enable
//   Bit 4: Tile data       (0=0x8800 signed, 1=0x8000 unsigned)
//   Bit 3: BG tilemap      (0=0x9800, 1=0x9C00)
//   Bit 2: Sprite size     (0=8×8,    1=8×16)
//   Bit 1: Sprites Enable
//   Bit 0: BG/Win Enable
//
// PALETAS
//   BGP  (0xFF47): bits 7-6 = cor 3, bits 5-4 = cor 2, bits 3-2 = cor 1, bits 1-0 = cor 0
//   OBP0 (0xFF48): igual, cor 0 = transparente nos sprites
//   OBP1 (0xFF49): igual
//
// FRAMEBUFFER
//   160 × 144 pixels, cada byte = sombra 0–3 (0=branco, 3=preto)
//   Salvo como PPM ao final — sem dependências externas.

// ─────────────────────────────────────────────────────────────────────────────
// Eventos emitidos pelo PPU num único tick; lidos pelo Bus para disparar IF.
// ─────────────────────────────────────────────────────────────────────────────
pub struct PpuEvents {
    pub vblank:       bool,
    pub stat_hblank:  bool,
    pub stat_vblank:  bool,
    pub stat_oam:     bool,
    pub stat_lyc:     bool,
}

impl PpuEvents {
    fn none() -> Self {
        Self { vblank: false, stat_hblank: false, stat_vblank: false,
               stat_oam: false, stat_lyc: false }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Struct principal
// ─────────────────────────────────────────────────────────────────────────────
pub struct Ppu {
    // ── timing ────────────────────────────────────────────────────────────────
    pub cycles:    u32,
    pub ly:        u8,
    pub lyc:       u8,
    prev_mode:     u8,

    // ── registradores LCD ─────────────────────────────────────────────────────
    pub lcdc:      u8,
    pub stat:      u8,
    pub scy:       u8,
    pub scx:       u8,
    pub wy:        u8,
    pub wx:        u8,
    pub bgp:       u8,
    pub obp0:      u8,
    pub obp1:      u8,

    // ── renderização ──────────────────────────────────────────────────────────
    /// Framebuffer 160×144, cada byte = sombra 0–3.
    pub framebuffer:  [u8; 160 * 144],
    /// Contador interno de linhas da Window (reseta a cada frame).
    window_line:      u8,
    /// True se a Window foi desenhada em alguma linha deste frame.
    window_triggered: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycles:           0,
            ly:               0,
            lyc:              0,
            prev_mode:        1,
            lcdc:             0x91,
            stat:             0x85,
            scy:              0,
            scx:              0,
            wy:               0,
            wx:               0,
            bgp:              0xFC,
            obp0:             0xFF,
            obp1:             0xFF,
            framebuffer:      [0u8; 160 * 144],
            window_line:      0,
            window_triggered: false,
        }
    }

    // =========================================================================
    // STEP — avança o PPU pelos `cycles` ciclos de uma instrução.
    // Recebe fatias de VRAM e OAM direto do bus.memory.
    // =========================================================================
    pub fn step(&mut self, cycles: u32, vram: &[u8], oam: &[u8]) -> PpuEvents {
        let mut ev = PpuEvents::none();

        // LCD desligado: congela timing, tela branca
        if self.lcdc & 0x80 == 0 {
            self.ly        = 0;
            self.cycles    = 0;
            self.stat      = self.stat & !0x03;
            self.prev_mode = 0;
            return ev;
        }

        self.cycles += cycles;

        // ── avança scanline ───────────────────────────────────────────────────
        if self.cycles >= 456 {
            self.cycles -= 456;
            let old_ly  = self.ly;
            self.ly     = (self.ly + 1) % 154;

            // Reset contador de frame ao voltar para linha 0
            if self.ly == 0 {
                self.window_line      = 0;
                self.window_triggered = false;
            }

            // Entrada em VBlank (linha 143 → 144)
            if old_ly == 143 && self.ly == 144 {
                ev.vblank = true;
                if self.stat & 0x10 != 0 { ev.stat_vblank = true; }
            }
        }

        // ── modo atual ────────────────────────────────────────────────────────
        let mode: u8 = if self.ly >= 144 { 1 } else {
            match self.cycles {
                0..=79   => 2,
                80..=251 => 3,
                _        => 0,
            }
        };

        // ── transições de modo ────────────────────────────────────────────────
        if mode != self.prev_mode {
            match mode {
                // Entrada em HBlank: hora de renderizar a scanline
                0 => {
                    if self.ly < 144 {
                        self.render_scanline(vram, oam);
                        // Incrementa linha da Window se ela foi desenhada nesta linha
                        if self.window_triggered {
                            self.window_line = self.window_line.saturating_add(1);
                        }
                        self.window_triggered = false;
                    }
                    if self.stat & 0x08 != 0 { ev.stat_hblank = true; }
                }
                2 => { if self.stat & 0x20 != 0 { ev.stat_oam = true; } }
                _ => {}
            }
            self.prev_mode = mode;
        }

        // ── atualiza STAT ─────────────────────────────────────────────────────
        self.stat = (self.stat & !0x03) | mode;

        // Bit 2: coincidência LY == LYC
        if self.ly == self.lyc {
            if self.stat & 0x04 == 0 {
                self.stat |= 0x04;
                if self.stat & 0x40 != 0 { ev.stat_lyc = true; }
            }
        } else {
            self.stat &= !0x04;
        }

        ev
    }

    // =========================================================================
    // RENDER_SCANLINE — chamado ao entrar em HBlank para cada linha 0–143
    // =========================================================================
    fn render_scanline(&mut self, vram: &[u8], oam: &[u8]) {
        let ly  = self.ly;
        let row = ly as usize * 160;

        // Buffer de prioridade do BG: true = pixel de cor ≠ 0 (bloqueia sprites com prioridade)
        let mut bg_prio = [false; 160];

        // ── 1. BG ─────────────────────────────────────────────────────────────
        if self.lcdc & 0x01 != 0 {
            self.render_bg(ly, vram, &mut bg_prio);
        } else {
            // BG desabilitado: tudo branco (cor 0)
            for x in 0..160 { self.framebuffer[row + x] = 0; }
        }

        // ── 2. Window ─────────────────────────────────────────────────────────
        // Window habilitada = LCDC bit 5 E bit 0
        if self.lcdc & 0x21 == 0x21 {
            self.render_window(ly, vram, &mut bg_prio);
        }

        // ── 3. Sprites ────────────────────────────────────────────────────────
        if self.lcdc & 0x02 != 0 {
            self.render_sprites(ly, vram, oam, &bg_prio);
        }
    }

    // =========================================================================
    // BACKGROUND
    // =========================================================================
    fn render_bg(&mut self, ly: u8, vram: &[u8], bg_prio: &mut [bool; 160]) {
        let row   = ly as usize * 160;
        // Tilemap: LCDC bit 3 → 0=0x9800 (vram[0x1800]), 1=0x9C00 (vram[0x1C00])
        let map_base: usize = if self.lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };

        for px in 0u8..160 {
            // Coordenada no mundo 256×256 (com wrap)
            let world_x = px.wrapping_add(self.scx);
            let world_y = ly.wrapping_add(self.scy);

            let tile_col = (world_x / 8) as usize;
            let tile_row = (world_y / 8) as usize;
            let tile_idx = map_base + tile_row * 32 + tile_col;

            let tile_num = vram[tile_idx];
            let fine_y   = (world_y % 8) as usize;
            let fine_x   = (7 - (world_x % 8)) as usize;  // bit mais alto = coluna 0

            let color_id = self.tile_pixel(vram, tile_num, fine_y, fine_x);
            let shade    = apply_palette(self.bgp, color_id);

            self.framebuffer[row + px as usize] = shade;
            bg_prio[px as usize] = color_id != 0;
        }
    }

    // =========================================================================
    // WINDOW
    // =========================================================================
    fn render_window(&mut self, ly: u8, vram: &[u8], bg_prio: &mut [bool; 160]) {
        // Window só aparece se LY >= WY
        if ly < self.wy { return; }

        // WX: posição horizontal + 7 (WX=7 → window começa em pixel 0)
        let wx = self.wx.saturating_sub(7);
        if wx >= 160 { return; }

        let row      = ly as usize * 160;
        let map_base = if self.lcdc & 0x40 != 0 { 0x1C00usize } else { 0x1800 };
        let win_y    = self.window_line as usize;

        for px in wx..160u8 {
            let win_x  = (px - wx) as usize;
            let tile_col = win_x / 8;
            let tile_row = win_y / 8;
            let tile_idx = map_base + tile_row * 32 + tile_col;

            let tile_num = vram[tile_idx];
            let fine_y   = win_y % 8;
            let fine_x   = 7 - (win_x % 8);

            let color_id = self.tile_pixel(vram, tile_num, fine_y, fine_x);
            let shade    = apply_palette(self.bgp, color_id);

            self.framebuffer[row + px as usize] = shade;
            bg_prio[px as usize] = color_id != 0;
            self.window_triggered = true;
        }
    }

    // =========================================================================
    // SPRITES
    // =========================================================================
    fn render_sprites(&mut self, ly: u8, vram: &[u8], oam: &[u8], bg_prio: &[bool; 160]) {
        let row  = ly as usize * 160;
        let tall = self.lcdc & 0x04 != 0; // sprite 8×16 quando bit 2 do LCDC setado
        let h    = if tall { 16i16 } else { 8i16 };
        let ly_i = ly as i16;

        // ── coleta os até 10 sprites visíveis nesta scanline ──────────────────
        // Prioridade de desenho no DMG: OAM menor = maior prioridade visual.
        // Iteramos de 0..40 e paramos em 10; depois desenhamos de trás para frente
        // para que o sprite de menor índice OAM sobrescreva os demais.
        let mut sprites: [(i16, i16, u8, u8); 10] = [(0,0,0,0); 10];
        let mut count = 0usize;

        for i in 0..40usize {
            let base  = i * 4;
            let spr_y = oam[base]     as i16 - 16; // Y real na tela
            let spr_x = oam[base + 1] as i16 - 8;  // X real na tela
            let tile  = oam[base + 2];
            let attr  = oam[base + 3];

            if ly_i >= spr_y && ly_i < spr_y + h {
                sprites[count] = (spr_y, spr_x, tile, attr);
                count += 1;
                if count == 10 { break; }
            }
        }

        // ── desenha de trás para frente (OAM maior idx = fundo) ──────────────
        for &(spr_y, spr_x, tile_raw, attr) in sprites[..count].iter().rev() {
            let flip_x  = attr & 0x20 != 0;
            let flip_y  = attr & 0x40 != 0;
            let behind  = attr & 0x80 != 0; // atrás do BG colors 1-3
            let palette = if attr & 0x10 != 0 { self.obp1 } else { self.obp0 };

            // Em 8×16 o bit 0 do tile é ignorado (hardware usa par/ímpar como unidade)
            let tile_num = if tall { tile_raw & 0xFE } else { tile_raw };

            // Linha do sprite que corresponde à scanline atual
            let mut row_in_spr = (ly_i - spr_y) as usize;
            if flip_y { row_in_spr = (h as usize - 1) - row_in_spr; }

            // Sprites apontam sempre para a região 0x8000 (endereçamento unsigned)
            let tile_addr = (tile_num as usize) * 16 + row_in_spr * 2;
            if tile_addr + 1 >= vram.len() { continue; }
            let lo = vram[tile_addr];
            let hi = vram[tile_addr + 1];

            for col in 0..8i16 {
                let screen_x = spr_x + col;
                if screen_x < 0 || screen_x >= 160 { continue; }
                let sx = screen_x as usize;

                // Com flip_x: coluna 0 usa bit 0, coluna 7 usa bit 7
                // Sem flip_x: coluna 0 usa bit 7, coluna 7 usa bit 0
                let bit_idx: usize = if flip_x { col as usize } else { 7 - col as usize };
                let color_id = (((hi >> bit_idx) & 1) << 1) | ((lo >> bit_idx) & 1);

                if color_id == 0 { continue; }         // cor 0 = transparente
                if behind && bg_prio[sx] { continue; } // sprite atrás do BG opaco

                self.framebuffer[row + sx] = apply_palette(palette, color_id);
            }
        }
    }

    // =========================================================================
    // TILE_PIXEL — decodifica 1 pixel de um tile na VRAM
    // =========================================================================
    // `tile_num`: índice do tile (unsigned ou signed dependendo de LCDC bit 4)
    // `fine_y`: linha dentro do tile (0–7)
    // `fine_x`: bit dentro do byte de linha (0–7, 7=esquerda, 0=direita)
    fn tile_pixel(&self, vram: &[u8], tile_num: u8, fine_y: usize, fine_x: usize) -> u8 {
        // Endereço base do tile dentro da VRAM
        let tile_addr: usize = if self.lcdc & 0x10 != 0 {
            // Modo unsigned: tile 0 → 0x8000 → vram[0]
            (tile_num as usize) * 16
        } else {
            // Modo signed: tile 0 → 0x9000 (vram[0x1000]), tile -1 → 0x8FF0
            let signed = tile_num as i8 as isize;
            (0x1000isize + signed * 16) as usize
        };

        let byte_lo = vram.get(tile_addr + fine_y * 2    ).copied().unwrap_or(0);
        let byte_hi = vram.get(tile_addr + fine_y * 2 + 1).copied().unwrap_or(0);

        ((( byte_hi >> fine_x) & 1) << 1) | ((byte_lo >> fine_x) & 1)
    }

    // =========================================================================
    // SALVA O FRAMEBUFFER EM FORMATO BMP 24-bit (sem dependências externas)
    // =========================================================================
    // BMP é suportado nativamente pelo VS Code, Windows Explorer e qualquer
    // visualizador de imagem — basta clicar no arquivo para ver o frame.
    //
    // Estrutura:
    //   14 bytes  File Header  (assinatura "BM" + tamanho + offset)
    //   40 bytes  DIB Header   (largura, altura, bpp, compressão...)
    //   N  bytes  Pixel data   (BGR, 3 bytes/pixel, linhas de baixo para cima)
    //
    // 160 × 3 = 480 bytes por linha — já alinhado a 4 bytes, sem padding.
    #[allow(dead_code)]
    pub fn save_bmp(&self, path: &str) -> std::io::Result<()> {
        // Paleta DMG-01 clássica: 4 tons de verde-cinza
        const PALETTE: [(u8, u8, u8); 4] = [
            (224, 248, 208), // 0 = branco-esverdeado
            (136, 192, 112), // 1 = verde claro
            ( 52, 104,  86), // 2 = verde escuro
            (  8,  24,  32), // 3 = quase preto
        ];

        const W: u32 = 160;
        const H: u32 = 144;
        const ROW_BYTES: u32 = W * 3;          // 480 — já múltiplo de 4
        let pixel_size: u32  = ROW_BYTES * H;  // 69.120 bytes
        let file_size:  u32  = 54 + pixel_size;

        let mut buf = Vec::with_capacity(file_size as usize);

        // ── File Header (14 bytes) ────────────────────────────────────────────
        buf.extend_from_slice(b"BM");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());  // reservado
        buf.extend_from_slice(&54u32.to_le_bytes()); // offset para pixels

        // ── DIB Header / BITMAPINFOHEADER (40 bytes) ─────────────────────────
        buf.extend_from_slice(&40u32.to_le_bytes());        // tamanho do header
        buf.extend_from_slice(&(W as i32).to_le_bytes());   // largura
        buf.extend_from_slice(&(H as i32).to_le_bytes());   // altura (positivo = linhas bottom-up)
        buf.extend_from_slice(&1u16.to_le_bytes());          // planos de cor
        buf.extend_from_slice(&24u16.to_le_bytes());         // bits por pixel
        buf.extend_from_slice(&0u32.to_le_bytes());          // compressão = BI_RGB
        buf.extend_from_slice(&0u32.to_le_bytes());          // tamanho dos pixels (0 ok para BI_RGB)
        buf.extend_from_slice(&0i32.to_le_bytes());          // pixels/metro X
        buf.extend_from_slice(&0i32.to_le_bytes());          // pixels/metro Y
        buf.extend_from_slice(&0u32.to_le_bytes());          // cores na tabela
        buf.extend_from_slice(&0u32.to_le_bytes());          // cores importantes

        // ── Pixel Data (BGR, bottom-up) ───────────────────────────────────────
        // BMP armazena linhas de baixo para cima: linha 143 primeiro
        for row in (0..144usize).rev() {
            for col in 0..160usize {
                let shade = self.framebuffer[row * 160 + col];
                let (r, g, b) = PALETTE[(shade & 3) as usize];
                buf.push(b); // ordem BMP = BGR
                buf.push(g);
                buf.push(r);
            }
        }

        std::fs::write(path, &buf)
    }

    // =========================================================================
    // REGISTRADORES LCD (leitura / escrita pelo Bus)
    // =========================================================================
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
            0xFF44 => { self.ly = 0; }   // escrita reseta LY
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

// ─────────────────────────────────────────────────────────────────────────────
// Aplica uma paleta de 8 bits a um índice de cor 0–3
// ─────────────────────────────────────────────────────────────────────────────
// Paleta: bits 7-6=cor3, bits 5-4=cor2, bits 3-2=cor1, bits 1-0=cor0
#[inline(always)]
fn apply_palette(palette: u8, color_id: u8) -> u8 {
    (palette >> (color_id * 2)) & 0x03
}
