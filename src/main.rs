mod bus;
mod cpu;
mod cartridge;
mod ppu;

use bus::Bus;
use cpu::Cpu;
use cartridge::Cartridge;
use minifb::{Key, Window, WindowOptions};

// =============================================================================
// CONSTANTES
// =============================================================================
const GB_W: usize = 160;
const GB_H: usize = 144;
const SCALE: usize = 3;                      // janela = 480 × 432
const WIN_W: usize = GB_W * SCALE;
const WIN_H: usize = GB_H * SCALE;
const CYCLES_PER_FRAME: u64 = 70_224;

// Paleta DMG-01 clássica em formato ARGB (0x00RRGGBB para minifb)
const PALETTE: [u32; 4] = [
    0x00E0F8D0, // 0 = branco-esverdeado
    0x0088C070, // 1 = verde claro
    0x00346856, // 2 = verde escuro
    0x00081820, // 3 = quase preto
];

// =============================================================================
// MAPEAMENTO DE TECLAS → BOTÕES DO GAME BOY
// =============================================================================
// D-pad  (bits 3-0 de joy.dpad):   Down=bit3, Up=bit2, Left=bit1, Right=bit0
// Botões (bits 3-0 de joy.buttons): Start=bit3, Sel=bit2, B=bit1, A=bit0
//
// Teclado:
//   Setas          → D-pad
//   Z              → A
//   X              → B
//   Enter          → Start
//   Backspace      → Select
//   Escape         → Sair

fn update_joypad(window: &Window, bus: &mut Bus) {
    // D-pad: 0 = pressionado, começa com tudo solto (0x0F)
    let mut dpad: u8 = 0x0F;
    if window.is_key_down(Key::Right) { dpad &= !0x01; }
    if window.is_key_down(Key::Left)  { dpad &= !0x02; }
    if window.is_key_down(Key::Up)    { dpad &= !0x04; }
    if window.is_key_down(Key::Down)  { dpad &= !0x08; }
    bus.joy.dpad = dpad;

    // Botões
    let mut buttons: u8 = 0x0F;
    if window.is_key_down(Key::Z)         { buttons &= !0x01; } // A
    if window.is_key_down(Key::X)         { buttons &= !0x02; } // B
    if window.is_key_down(Key::Backspace) { buttons &= !0x04; } // Select
    if window.is_key_down(Key::Enter)     { buttons &= !0x08; } // Start
    bus.joy.buttons = buttons;
}

// =============================================================================
// CONVERTE framebuffer Game Boy (u8 shades 0-3) para buffer ARGB upscalado
// =============================================================================
fn blit_framebuffer(fb: &[u8; GB_W * GB_H], out: &mut Vec<u32>) {
    for gy in 0..GB_H {
        for gx in 0..GB_W {
            let color = PALETTE[(fb[gy * GB_W + gx] & 3) as usize];
            // Upscale SCALE×SCALE
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let idx = (gy * SCALE + dy) * WIN_W + (gx * SCALE + dx);
                    out[idx] = color;
                }
            }
        }
    }
}

// =============================================================================
// LOOP PRINCIPAL — roda um frame completo (70.224 ciclos) por iteração de janela
// =============================================================================
fn run_frame(cpu: &mut Cpu, bus: &mut Bus) {
    let mut frame_cycles: u64 = 0;

    while frame_cycles < CYCLES_PER_FRAME {
        // Despacha interrupções
        let int_cyc = cpu.handle_interrupts(bus);
        if int_cyc > 0 {
            bus.tick(int_cyc);
            frame_cycles += int_cyc as u64;
            continue;
        }

        // HALT: aguarda interrupção
        if cpu.halted {
            bus.tick(4);
            frame_cycles += 4;
            continue;
        }

        // Executa instrução
        let cycles = cpu.step(bus);
        bus.tick(cycles);
        frame_cycles += cycles as u64;
    }
}

// =============================================================================
// MAIN
// =============================================================================
fn main() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();
    cpu.verbose = false; // janela em tempo real — sem log no terminal

    println!("--- GAME BOY DMG-01 ---");

    match Cartridge::load("teste.gb") {
        Ok(cartridge) => {
            bus.connect_cartridge(cartridge);
        }
        Err(e) => {
            eprintln!("Erro ao carregar ROM: {}", e);
            return;
        }
    }

    // ── Cria janela ───────────────────────────────────────────────────────────
    let mut window = match Window::new(
        "Game Boy — Pokémon Red  |  Z=A  X=B  Enter=Start  Backspace=Select  Setas=D-pad  Esc=Sair",
        WIN_W,
        WIN_H,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    ) {
        Ok(w)  => w,
        Err(e) => { eprintln!("Erro ao criar janela: {}", e); return; }
    };

    // Limita a taxa de atualização ao clock real do DMG-01: ~59,73 fps
    // 1 / 59,7275 Hz ≈ 16.742 µs por frame
    window.limit_update_rate(Some(std::time::Duration::from_micros(16_742)));

    let mut pixel_buf = vec![0u32; WIN_W * WIN_H];

    println!("Janela aberta. Pressione Esc para sair.");
    println!("Controles: Z=A  X=B  Enter=Start  Backspace=Select  Setas=D-pad");

    // ── Game loop ─────────────────────────────────────────────────────────────
    while window.is_open() && !window.is_key_down(Key::Escape) {
        // 1. Lê teclado → atualiza joypad
        update_joypad(&window, &mut bus);

        // 2. Roda um frame completo (70.224 ciclos de clock)
        run_frame(&mut cpu, &mut bus);

        // 3. Converte e upscala o framebuffer para o buffer da janela
        blit_framebuffer(&bus.ppu.framebuffer, &mut pixel_buf);

        // 4. Apresenta na janela
        window
            .update_with_buffer(&pixel_buf, WIN_W, WIN_H)
            .unwrap_or_else(|e| eprintln!("Erro ao atualizar janela: {}", e));
    }

    println!("Encerrando emulador.");
}
