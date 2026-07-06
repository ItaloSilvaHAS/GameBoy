use crate::cartridge::Cartridge;

pub struct Bus {
    pub memory: [u8; 65536],
    pub cartridge: Option<Cartridge>, // Pode ou não ter um cartucho inserido
}

impl Bus {
    pub fn new() -> Self {
        Self {
            memory: [0; 65536],
            cartridge: None,
        }
    }

    // Conecta o cartucho ao barramento
    pub fn connect_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(cartridge);
    }

    pub fn read(&self, address: u16) -> u8 {
        // Se o endereço estiver na faixa da ROM (0x0000 até 0x7FFF)
        if address <= 0x7FFF {
            if let Some(ref cart) = self.cartridge {
                return cart.read(address);
            }
        }
        // Caso contrário, lê da RAM normal do sistema
        self.memory[address as usize]
    }

    pub fn write(&mut self, address: u16, value: u8) {
        // O Game Boy não permite escrever na ROM diretamente, então só escrevemos se for na RAM (acima de 0x7FFF)
        if address > 0x7FFF {
            self.memory[address as usize] = value;
        }
    }
}