use crate::bus::Bus;

pub struct Cpu {
    pub a: u8,
    pub b: u8,
    pub pc: u16, // Program Counter
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            pc: 0, // Começamos no endereço 0 para o nosso teste básico
        }
    }

    // Executa uma única instrução
    pub fn step(&mut self, bus: &mut Bus) {
        // 1. FETCH (Busca a instrução na memória)
        let opcode = bus.read(self.pc);
        println!("PC: {:#06X} | Executando Opcode: {:#04X}", self.pc, opcode);
        
        // Avança o PC para a próxima posição
        self.pc += 1;

        // 2. DECODE & EXECUTE (Decodifica e Executa)
        match opcode {
            0x00 => {
                // NOP (No Operation) - Não faz nada, só passa o tempo
                println!("-> Instrução: NOP");
            }
            0x3C => {
                // INC A (Incrementa o registrador A)
                self.a = self.a.wrapping_add(1);
                println!("-> Instrução: INC A | Novo valor de A: {}", self.a);
            }
            0x06 => {
                // LD B, d8 (Carrega um valor de 8 bits imediato no registrador B)
                let value = bus.read(self.pc);
                self.b = value;
                self.pc += 1; // Avança o PC porque consumimos o valor
                println!("-> Instrução: LD B, {} | Novo valor de B: {}", value, self.b);
            }
            0xC3 => {
                // JP nn (Jump para um endereço de 16 bits)
                // O Game Boy armazena endereços em Little Endian (o byte menos significativo vem primeiro)
                let low_byte = bus.read(self.pc) as u16;
                let high_byte = bus.read(self.pc + 1) as u16;
                
                // Junta os dois bytes de 8 bits em um único endereço de 16 bits
                let target_address = (high_byte << 8) | low_byte;

                // Movemos o PC direto para o destino do pulo (consome os 2 bytes extras)
                self.pc = target_address;
                println!("-> Instrução: JP {:#06X} (Pulando para este endereço!)", target_address);
            }
            _ => {
                println!("-> Opcode desconhecido ou não implementado! Travando a CPU.");
            }
        }
    }
}