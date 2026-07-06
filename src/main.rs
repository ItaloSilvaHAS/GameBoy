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

    // 3 frames completos: 3 × 70.224 ciclos = ~210.672 ciclos
    let max_cycles: u64 = 70_224 * 3;
    let mut total_cycles: u64 = 0;
    let mut total_steps:  u64 = 0;

    // Detector de loop de polling:
    // Se o PC se repetir muitas vezes seguidas, silencia o log para não
    // inundar o terminal. Quando o PC sair do loop, volta a logar.
    let mut last_unique_pc: u16 = 0xFFFF;
    let mut repeat_count:   u32 = 0;
    const QUIET_AFTER: u32 = 6; // silencia após 6 repetições no mesmo PC

    while total_cycles < max_cycles {
        if cpu.halted {
            println!("\n=== CPU em HALT ({} ciclos, {} instruções) ===",
                total_cycles, total_steps);
            break;
        }
        if cpu.stopped {
            println!("\n=== CPU em STOP ({} ciclos, {} instruções) ===",
                total_cycles, total_steps);
            break;
        }

        let pc_before = cpu.pc;

        // Atualiza o detector de loop
        if pc_before == last_unique_pc {
            repeat_count += 1;
            if repeat_count == QUIET_AFTER {
                println!("  [entrando em loop de polling em PC={:#06X}, silenciando log...]", pc_before);
            }
            cpu.verbose = repeat_count < QUIET_AFTER;
        } else {
            if repeat_count >= QUIET_AFTER {
                println!("  [saiu do loop após {} iterações | LY={} ciclos_totais={}]",
                    repeat_count, bus.ppu.ly, total_cycles);
            }
            repeat_count       = 0;
            last_unique_pc     = pc_before;
            cpu.verbose        = true;
        }

        let cycles = cpu.step(&mut bus);
        bus.ppu.step(cycles);
        total_cycles += cycles as u64;
        total_steps  += 1;

        // PC não avançou e não está em HALT = opcode não implementado
        if cpu.pc == pc_before && !cpu.halted {
            cpu.verbose = true;
            println!("\n=== TRAVADO: opcode não implementado em PC={:#06X} ===", pc_before);
            println!("=== {} ciclos | {} instruções ===", total_cycles, total_steps);
            // Mostra estado completo da CPU ao travar
            println!("A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:#06X}",
                cpu.a, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l, cpu.sp);
            println!("Flags: Z={} N={} H={} C={}",
                cpu.f.zero as u8, cpu.f.subtract as u8,
                cpu.f.half_carry as u8, cpu.f.carry as u8);
            break;
        }
    }

    if total_cycles >= max_cycles {
        println!("\n=== Limite de 3 frames atingido ===");
        println!("PC={:#06X} | LY={} | {} ciclos | {} instruções",
            cpu.pc, bus.ppu.ly, total_cycles, total_steps);
        println!("A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:#06X}",
            cpu.a, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l, cpu.sp);
    }

    println!("\n--- FIM ---");
}
