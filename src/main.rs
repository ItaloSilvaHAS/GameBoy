mod bus;
mod cpu;

use bus::Bus;
use cpu::Cpu;

fn main() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();

    println!("--- INICIALIZANDO EMULADOR MEGA BÁSICO ---");

    // Simulando uma "ROM" injetando bytes direto na memória no endereço 0x0000
    // 0x00 = NOP
    // 0x3C = INC A
    // 0x06 = LD B, d8 (o próximo byte é o valor)
    // 0x42 = O valor que vai para o registrador B
    bus.write(0x0000, 0x00); // Passo 1: NOP
    bus.write(0x0001, 0x3C); // Passo 2: INC A
    bus.write(0x0002, 0x06); // Passo 3: LD B, 42
    bus.write(0x0003, 0x42); // Dado para o comando anterior

    // Loop de execução (vamos rodar 3 passos/clocks de teste)
    for _ in 0..3 {
        cpu.step(&mut bus);
        println!("---------------------------------------");
    }
}