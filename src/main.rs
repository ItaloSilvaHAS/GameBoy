mod bus;
mod cpu;
mod cartridge;

use bus::Bus;
use cpu::Cpu;
use cartridge::Cartridge;

fn main() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();

    println!("--- INICIALIZANDO EMULADOR COM ROM REAL ---");

    // Tenta carregar a ROM (vamos criar um arquivo chamado "teste.gb" na raiz do projeto)
    match Cartridge::load("teste.gb") {
        Ok(cartridge) => {
            bus.connect_cartridge(cartridge);
            println!("Cartucho conectado ao barramento.");
        }
        Err(e) => {
            println!("Erro ao carregar a ROM 'teste.gb': {}. Certifique-se de que o arquivo existe na raiz do projeto.", e);
            return;
        }
    }

    // O Game Boy real, após terminar o boot interno dele, começa a executar
    // o jogo exatamente no endereço 0x0100 do Program Counter.
    cpu.pc = 0x0100;

    // Vamos rodar 5 ciclos para ver o que o jogo de verdade faz primeiro!
    for _ in 0..5 {
        cpu.step(&mut bus);
        println!("---------------------------------------");
    }
}