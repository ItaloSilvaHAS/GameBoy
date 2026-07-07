mod bus;
mod cpu;
mod cartridge;
mod ppu;

use bus::Bus;
use cpu::Cpu;
use cartridge::Cartridge;

fn main() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();

    println!("--- INICIALIZANDO EMULADOR ---");

    match Cartridge::load("teste.gb") {
        Ok(cartridge) => {
            bus.connect_cartridge(cartridge);
            println!("Cartucho conectado. Iniciando em 0x0100.\n");
        }
        Err(e) => {
            println!("Erro ao carregar ROM: {}", e);
            return;
        }
    }

    // 60 frames (~4.2 milhões de ciclos) — o suficiente para o jogo terminar
    // toda a inicialização e entrar no loop principal
    let max_cycles: u64 = 70_224 * 60;
    let mut total_cycles: u64 = 0;
    let mut total_steps:  u64 = 0;

    // Detector de loop de polling silencioso
    let mut last_unique_pc: u16 = 0xFFFF;
    let mut repeat_count:   u32 = 0;
    const QUIET_AFTER: u32 = 6;

    while total_cycles < max_cycles {
        // ── 1. Despacha interrupções pendentes (antes do fetch) ──────────────
        let int_cycles = cpu.handle_interrupts(&mut bus);
        if int_cycles > 0 {
            bus.tick(int_cycles);
            total_cycles += int_cycles as u64;
            // Reseta detector de loop: saímos do contexto atual
            repeat_count   = 0;
            last_unique_pc = cpu.pc;
            cpu.verbose    = true;
            continue;
        }

        // ── 2. HALT: CPU dorme até próxima interrupção ───────────────────────
        if cpu.halted {
            // Avança 4 ciclos por vez enquanto aguarda
            bus.tick(4);
            total_cycles += 4;
            continue;
        }

        // ── 3. Detecta loop de polling e controla verbosidade ─────────────────
        let pc_before = cpu.pc;
        if pc_before == last_unique_pc {
            repeat_count += 1;
            if repeat_count == QUIET_AFTER {
                println!("  [loop de polling em PC={:#06X}, silenciando...]", pc_before);
            }
            cpu.verbose = repeat_count < QUIET_AFTER;
        } else {
            if repeat_count >= QUIET_AFTER {
                println!("  [saiu do loop após {} iterações | LY={} | ciclos={}]",
                    repeat_count, bus.ppu.ly, total_cycles);
            }
            repeat_count       = 0;
            last_unique_pc     = pc_before;
            cpu.verbose        = true;
        }

        // ── 4. Executa instrução ──────────────────────────────────────────────
        let cycles = cpu.step(&mut bus);
        bus.tick(cycles);
        total_cycles += cycles as u64;
        total_steps  += 1;

        // ── 5. Trava de segurança: opcode não implementado ────────────────────
        if cpu.pc == pc_before && !cpu.halted {
            cpu.verbose = true;
            println!("\n=== TRAVADO: opcode não implementado em PC={:#06X} ===", pc_before);
            println!("    Opcode: {:#04X}", bus.read(pc_before));
            println!("    {} ciclos | {} instruções | LY={}",
                total_cycles, total_steps, bus.ppu.ly);
            println!("    A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:#06X}",
                cpu.a, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l, cpu.sp);
            println!("    Flags: Z={} N={} H={} C={}",
                cpu.f.zero as u8, cpu.f.subtract as u8,
                cpu.f.half_carry as u8, cpu.f.carry as u8);
            break;
        }
    }

    if total_cycles >= max_cycles {
        println!("\n=== {} frames concluídos ===", max_cycles / 70_224);
        println!("    PC={:#06X} | LY={} | {} ciclos | {} instruções",
            cpu.pc, bus.ppu.ly, total_cycles, total_steps);
        println!("    A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:#06X}",
            cpu.a, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l, cpu.sp);
    }

    println!("\n--- FIM ---");
}
