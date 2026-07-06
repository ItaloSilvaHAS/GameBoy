pub struct Bus {
    // O Game Boy clássico tem um mapa de memória de 65.536 bytes (64KB)
    pub memory: [u8; 65536],
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory: [0; 65536],
        }
    }

    // Lê um byte de um endereço de 16 bits
    pub fn read(&self, address: u16) -> u8 {
        self.memory[address as usize]
    }

    // Escreve um byte em um endereço de 16 bits
    pub fn write(&mut self, address: u16, value: u8) {
        self.memory[address as usize] = value;
    }
}