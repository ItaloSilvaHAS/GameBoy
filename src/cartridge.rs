use std::fs::File;
use std::io::Read;
use std::path::Path;

pub struct Cartridge {
    pub rom: Vec<u8>,
}

impl Cartridge {
    // Função que carrega o arquivo da ROM para a memória do emulador
    pub fn load<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut rom = Vec::new();
        file.read_to_end(&mut rom)?;

        println!("-> ROM Carregada com sucesso! Tamanho: {} bytes", rom.len());
        Ok(Self { rom })
    }

    // Lê um byte do cartucho
    pub fn read(&self, address: u16) -> u8 {
        // Por enquanto, vamos mapear de forma direta
        // O Game Boy lê a ROM nos endereços de 0x0000 a 0x7FFF
        if (address as usize) < self.rom.len() {
            self.rom[address as usize]
        } else {
            0
        }
    }
}