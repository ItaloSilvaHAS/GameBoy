use crate::bus::Bus;

// =============================================================================
// REGISTRADOR DE FLAGS (Registrador F)
// =============================================================================
//  Bit 7 (Z) - Zero Flag:        Setado quando o resultado de uma operação é 0.
//  Bit 6 (N) - Subtraction Flag: Setado quando a última operação foi uma subtração.
//  Bit 5 (H) - Half-Carry Flag:  Carry do nibble baixo para o alto (bit 3 -> 4).
//  Bit 4 (C) - Carry Flag:       Overflow/underflow de 8 bits.
//  Bits 3-0:  Sempre 0.
//
//  [ Z | N | H | C | 0 | 0 | 0 | 0 ]
//    7   6   5   4   3   2   1   0

#[allow(dead_code)]
pub struct Flags {
    pub zero:       bool, // Z
    pub subtract:   bool, // N
    pub half_carry: bool, // H
    pub carry:      bool, // C
}

impl Flags {
    pub fn new() -> Self {
        // Estado do DMG-01 após o boot ROM interno: F = 0xB0
        Self {
            zero:       true,
            subtract:   false,
            half_carry: true,
            carry:      true,
        }
    }

    pub fn to_byte(&self) -> u8 {
        let mut f = 0u8;
        if self.zero       { f |= 1 << 7; }
        if self.subtract   { f |= 1 << 6; }
        if self.half_carry { f |= 1 << 5; }
        if self.carry      { f |= 1 << 4; }
        f
    }

    pub fn from_byte(&mut self, byte: u8) {
        self.zero       = (byte & (1 << 7)) != 0;
        self.subtract   = (byte & (1 << 6)) != 0;
        self.half_carry = (byte & (1 << 5)) != 0;
        self.carry      = (byte & (1 << 4)) != 0;
    }
}

// =============================================================================
// ESTRUTURA PRINCIPAL DA CPU (Sharp SM83)
// =============================================================================

#[allow(dead_code)]
pub struct Cpu {
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub f: Flags,
    pub sp:       u16,
    pub pc:       u16,
    pub halted:   bool,
    pub stopped:  bool,
    pub ime:      bool, // Interrupt Master Enable
    pub ime_next: bool, // EI tem delay de 1 instrução
    pub verbose:  bool, // false = silencia os println! (útil em loops de polling)
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            a:  0x01,
            b:  0x00,
            c:  0x13,
            d:  0x00,
            e:  0xD8,
            h:  0x01,
            l:  0x4D,
            f:  Flags::new(),
            sp:       0xFFFE,
            pc:       0x0100,
            halted:   false,
            stopped:  false,
            ime:      false,
            ime_next: false,
            verbose:  true,
        }
    }

    // =========================================================================
    // PARES DE REGISTRADORES (16 bits)
    // =========================================================================

    pub fn bc(&self) -> u16 { ((self.b as u16) << 8) | (self.c as u16) }
    pub fn set_bc(&mut self, v: u16) { self.b = (v >> 8) as u8; self.c = (v & 0xFF) as u8; }

    pub fn de(&self) -> u16 { ((self.d as u16) << 8) | (self.e as u16) }
    pub fn set_de(&mut self, v: u16) { self.d = (v >> 8) as u8; self.e = (v & 0xFF) as u8; }

    pub fn hl(&self) -> u16 { ((self.h as u16) << 8) | (self.l as u16) }
    pub fn set_hl(&mut self, v: u16) { self.h = (v >> 8) as u8; self.l = (v & 0xFF) as u8; }

    pub fn af(&self) -> u16 { ((self.a as u16) << 8) | (self.f.to_byte() as u16) }
    pub fn set_af(&mut self, v: u16) { self.a = (v >> 8) as u8; self.f.from_byte((v & 0xFF) as u8); }

    // =========================================================================
    // FETCH HELPERS
    // =========================================================================

    fn fetch_byte(&mut self, bus: &Bus) -> u8 {
        let byte = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        byte
    }

    fn fetch_word(&mut self, bus: &Bus) -> u16 {
        let lo = self.fetch_byte(bus) as u16;
        let hi = self.fetch_byte(bus) as u16;
        (hi << 8) | lo
    }

    // =========================================================================
    // STACK HELPERS
    // =========================================================================

    pub fn stack_push(&mut self, bus: &mut Bus, value: u16) {
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, (value >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, (value & 0xFF) as u8);
    }

    pub fn stack_pop(&mut self, bus: &mut Bus) -> u16 {
        let lo = bus.read(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = bus.read(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        (hi << 8) | lo
    }

    // =========================================================================
    // HELPERS DE ALU (Aritmética e Lógica)
    // =========================================================================

    fn alu_add(&mut self, val: u8) {
        let a = self.a;
        let result = a.wrapping_add(val);
        self.f.zero       = result == 0;
        self.f.subtract   = false;
        self.f.half_carry = (a & 0x0F) + (val & 0x0F) > 0x0F;
        self.f.carry      = (a as u16) + (val as u16) > 0xFF;
        self.a = result;
    }

    fn alu_adc(&mut self, val: u8) {
        let a = self.a;
        let c = self.f.carry as u8;
        let result = a.wrapping_add(val).wrapping_add(c);
        self.f.zero       = result == 0;
        self.f.subtract   = false;
        self.f.half_carry = (a & 0x0F) + (val & 0x0F) + c > 0x0F;
        self.f.carry      = (a as u16) + (val as u16) + (c as u16) > 0xFF;
        self.a = result;
    }

    fn alu_sub(&mut self, val: u8) {
        let a = self.a;
        let result = a.wrapping_sub(val);
        self.f.zero       = result == 0;
        self.f.subtract   = true;
        self.f.half_carry = (a & 0x0F) < (val & 0x0F);
        self.f.carry      = (a as u16) < (val as u16);
        self.a = result;
    }

    fn alu_sbc(&mut self, val: u8) {
        let a = self.a;
        let c = self.f.carry as u8;
        let result = a.wrapping_sub(val).wrapping_sub(c);
        self.f.zero       = result == 0;
        self.f.subtract   = true;
        self.f.half_carry = (a & 0x0F) < (val & 0x0F) + c;
        self.f.carry      = (a as u16) < (val as u16) + (c as u16);
        self.a = result;
    }

    fn alu_and(&mut self, val: u8) {
        self.a &= val;
        self.f.zero       = self.a == 0;
        self.f.subtract   = false;
        self.f.half_carry = true;
        self.f.carry      = false;
    }

    fn alu_xor(&mut self, val: u8) {
        self.a ^= val;
        self.f.zero       = self.a == 0;
        self.f.subtract   = false;
        self.f.half_carry = false;
        self.f.carry      = false;
    }

    fn alu_or(&mut self, val: u8) {
        self.a |= val;
        self.f.zero       = self.a == 0;
        self.f.subtract   = false;
        self.f.half_carry = false;
        self.f.carry      = false;
    }

    fn alu_cp(&mut self, val: u8) {
        let a = self.a;
        self.f.zero       = a == val;
        self.f.subtract   = true;
        self.f.half_carry = (a & 0x0F) < (val & 0x0F);
        self.f.carry      = a < val;
    }

    fn alu_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        self.f.zero       = result == 0;
        self.f.subtract   = false;
        self.f.half_carry = (val & 0x0F) == 0x0F;
        result
    }

    fn alu_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        self.f.zero       = result == 0;
        self.f.subtract   = true;
        self.f.half_carry = (val & 0x0F) == 0x00;
        result
    }

    fn alu_add_hl(&mut self, val: u16) {
        let hl = self.hl();
        let result = hl.wrapping_add(val);
        self.f.subtract   = false;
        self.f.half_carry = (hl & 0x0FFF) + (val & 0x0FFF) > 0x0FFF;
        self.f.carry      = (hl as u32) + (val as u32) > 0xFFFF;
        self.set_hl(result);
    }

    // Lê registrador pelo índice SM83: 0=B 1=C 2=D 3=E 4=H 5=L 6=(HL) 7=A
    fn read_r8(&self, idx: u8, bus: &Bus) -> u8 {
        match idx {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => bus.read(self.hl()),
            7 => self.a,
            _ => unreachable!(),
        }
    }

    fn write_r8(&mut self, idx: u8, val: u8, bus: &mut Bus) {
        match idx {
            0 => self.b = val,
            1 => self.c = val,
            2 => self.d = val,
            3 => self.e = val,
            4 => self.h = val,
            5 => self.l = val,
            6 => { let addr = self.hl(); bus.write(addr, val); }
            7 => self.a = val,
            _ => unreachable!(),
        }
    }

    fn r8_name(idx: u8) -> &'static str {
        match idx {
            0 => "B", 1 => "C", 2 => "D", 3 => "E",
            4 => "H", 5 => "L", 6 => "(HL)", 7 => "A",
            _ => "?",
        }
    }

    // =========================================================================
    // CICLO PRINCIPAL: FETCH -> DECODE -> EXECUTE
    // Retorna o número de ciclos de clock consumidos pela instrução.
    // =========================================================================
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Macro local: só printa se verbose = true
        macro_rules! log {
            ($($arg:tt)*) => { if self.verbose { println!($($arg)*); } }
        }

        // Processar delay do EI (habilita IME após 1 instrução)
        if self.ime_next {
            self.ime_next = false;
            self.ime = true;
        }

        if self.halted {
            return 4;
        }

        let pc_before = self.pc;
        let opcode = self.fetch_byte(bus);

        log!(
            "PC:{:#06X} | {:#04X} | A:{:02X} BC:{:02X}{:02X} DE:{:02X}{:02X} HL:{:02X}{:02X} SP:{:#06X} | Z:{} N:{} H:{} C:{}",
            pc_before, opcode,
            self.a, self.b, self.c, self.d, self.e, self.h, self.l, self.sp,
            self.f.zero as u8, self.f.subtract as u8,
            self.f.half_carry as u8, self.f.carry as u8
        );

        match opcode {

            // -----------------------------------------------------------------
            // MISC / CONTROLE
            // -----------------------------------------------------------------

            0x00 => { log!("  NOP"); 4 }

            0x10 => {
                self.fetch_byte(bus); // consome o 0x00 seguinte
                self.stopped = true;
                log!("  STOP");
                4
            }

            0x76 => {
                self.halted = true;
                log!("  HALT");
                4
            }

            0x27 => {
                // DAA — ajuste decimal do acumulador (BCD)
                let mut a = self.a;
                if !self.f.subtract {
                    if self.f.half_carry || (a & 0x0F) > 9 { a = a.wrapping_add(0x06); }
                    if self.f.carry || a > 0x99 { a = a.wrapping_add(0x60); self.f.carry = true; }
                } else {
                    if self.f.half_carry { a = a.wrapping_sub(0x06); }
                    if self.f.carry      { a = a.wrapping_sub(0x60); }
                }
                self.f.zero       = a == 0;
                self.f.half_carry = false;
                self.a = a;
                log!("  DAA");
                4
            }

            0x2F => {
                // CPL — complemento bitwise de A
                self.a = !self.a;
                self.f.subtract   = true;
                self.f.half_carry = true;
                log!("  CPL");
                4
            }

            0x37 => {
                // SCF — seta o Carry Flag
                self.f.subtract   = false;
                self.f.half_carry = false;
                self.f.carry      = true;
                log!("  SCF");
                4
            }

            0x3F => {
                // CCF — inverte o Carry Flag
                self.f.subtract   = false;
                self.f.half_carry = false;
                self.f.carry      = !self.f.carry;
                log!("  CCF");
                4
            }

            0xF3 => { self.ime = false;      log!("  DI"); 4 }
            0xFB => { self.ime_next = true;  log!("  EI"); 4 }

            // -----------------------------------------------------------------
            // ROTAÇÕES DO ACUMULADOR (sem prefixo CB)
            // -----------------------------------------------------------------

            0x07 => {
                // RLCA
                let c = self.a >> 7;
                self.a = (self.a << 1) | c;
                self.f.zero = false; self.f.subtract = false; self.f.half_carry = false;
                self.f.carry = c != 0;
                log!("  RLCA");
                4
            }
            0x0F => {
                // RRCA
                let c = self.a & 1;
                self.a = (self.a >> 1) | (c << 7);
                self.f.zero = false; self.f.subtract = false; self.f.half_carry = false;
                self.f.carry = c != 0;
                log!("  RRCA");
                4
            }
            0x17 => {
                // RLA
                let old_carry = self.f.carry as u8;
                let new_carry = self.a >> 7;
                self.a = (self.a << 1) | old_carry;
                self.f.zero = false; self.f.subtract = false; self.f.half_carry = false;
                self.f.carry = new_carry != 0;
                log!("  RLA");
                4
            }
            0x1F => {
                // RRA
                let old_carry = self.f.carry as u8;
                let new_carry = self.a & 1;
                self.a = (self.a >> 1) | (old_carry << 7);
                self.f.zero = false; self.f.subtract = false; self.f.half_carry = false;
                self.f.carry = new_carry != 0;
                log!("  RRA");
                4
            }

            // -----------------------------------------------------------------
            // LD rr, d16 — carrega imediato de 16 bits num par de registradores
            // -----------------------------------------------------------------

            0x01 => { let v = self.fetch_word(bus); self.set_bc(v); log!("  LD BC, {:#06X}", v); 12 }
            0x11 => { let v = self.fetch_word(bus); self.set_de(v); log!("  LD DE, {:#06X}", v); 12 }
            0x21 => { let v = self.fetch_word(bus); self.set_hl(v); log!("  LD HL, {:#06X}", v); 12 }
            0x31 => { let v = self.fetch_word(bus); self.sp = v;    log!("  LD SP, {:#06X}", v); 12 }

            // LD (nn), SP
            0x08 => {
                let addr = self.fetch_word(bus);
                bus.write(addr,     (self.sp & 0xFF) as u8);
                bus.write(addr + 1, (self.sp >> 8)   as u8);
                log!("  LD ({:#06X}), SP", addr);
                20
            }

            // LD SP, HL
            0xF9 => { self.sp = self.hl(); log!("  LD SP, HL"); 8 }

            // LD HL, SP+r8
            0xF8 => {
                let offset = self.fetch_byte(bus) as i8 as i16 as u16;
                let result = self.sp.wrapping_add(offset);
                self.f.zero       = false;
                self.f.subtract   = false;
                self.f.half_carry = (self.sp & 0x000F) + (offset & 0x000F) > 0x000F;
                self.f.carry      = (self.sp & 0x00FF) + (offset & 0x00FF) > 0x00FF;
                self.set_hl(result);
                log!("  LD HL, SP+{:+}", offset as i16 as i8);
                12
            }

            // ADD SP, r8
            0xE8 => {
                let offset = self.fetch_byte(bus) as i8 as i16 as u16;
                self.f.zero       = false;
                self.f.subtract   = false;
                self.f.half_carry = (self.sp & 0x000F) + (offset & 0x000F) > 0x000F;
                self.f.carry      = (self.sp & 0x00FF) + (offset & 0x00FF) > 0x00FF;
                self.sp = self.sp.wrapping_add(offset);
                log!("  ADD SP, {:+}", offset as i16 as i8);
                16
            }

            // -----------------------------------------------------------------
            // INC / DEC rr
            // -----------------------------------------------------------------

            0x03 => { let v = self.bc().wrapping_add(1); self.set_bc(v); log!("  INC BC"); 8 }
            0x13 => { let v = self.de().wrapping_add(1); self.set_de(v); log!("  INC DE"); 8 }
            0x23 => { let v = self.hl().wrapping_add(1); self.set_hl(v); log!("  INC HL"); 8 }
            0x33 => { self.sp = self.sp.wrapping_add(1);                  log!("  INC SP"); 8 }

            0x0B => { let v = self.bc().wrapping_sub(1); self.set_bc(v); log!("  DEC BC"); 8 }
            0x1B => { let v = self.de().wrapping_sub(1); self.set_de(v); log!("  DEC DE"); 8 }
            0x2B => { let v = self.hl().wrapping_sub(1); self.set_hl(v); log!("  DEC HL"); 8 }
            0x3B => { self.sp = self.sp.wrapping_sub(1);                  log!("  DEC SP"); 8 }

            // -----------------------------------------------------------------
            // ADD HL, rr
            // -----------------------------------------------------------------

            0x09 => { let v = self.bc(); self.alu_add_hl(v); log!("  ADD HL, BC"); 8 }
            0x19 => { let v = self.de(); self.alu_add_hl(v); log!("  ADD HL, DE"); 8 }
            0x29 => { let v = self.hl(); self.alu_add_hl(v); log!("  ADD HL, HL"); 8 }
            0x39 => { let v = self.sp;   self.alu_add_hl(v); log!("  ADD HL, SP"); 8 }

            // -----------------------------------------------------------------
            // INC r8
            // -----------------------------------------------------------------

            0x04 => { let v = self.alu_inc(self.b); self.b = v; log!("  INC B"); 4 }
            0x0C => { let v = self.alu_inc(self.c); self.c = v; log!("  INC C"); 4 }
            0x14 => { let v = self.alu_inc(self.d); self.d = v; log!("  INC D"); 4 }
            0x1C => { let v = self.alu_inc(self.e); self.e = v; log!("  INC E"); 4 }
            0x24 => { let v = self.alu_inc(self.h); self.h = v; log!("  INC H"); 4 }
            0x2C => { let v = self.alu_inc(self.l); self.l = v; log!("  INC L"); 4 }
            0x34 => {
                let addr = self.hl();
                let val  = bus.read(addr);
                let v    = self.alu_inc(val);
                bus.write(addr, v);
                log!("  INC (HL)");
                12
            }
            0x3C => { let v = self.alu_inc(self.a); self.a = v; log!("  INC A"); 4 }

            // -----------------------------------------------------------------
            // DEC r8
            // -----------------------------------------------------------------

            0x05 => { let v = self.alu_dec(self.b); self.b = v; log!("  DEC B"); 4 }
            0x0D => { let v = self.alu_dec(self.c); self.c = v; log!("  DEC C"); 4 }
            0x15 => { let v = self.alu_dec(self.d); self.d = v; log!("  DEC D"); 4 }
            0x1D => { let v = self.alu_dec(self.e); self.e = v; log!("  DEC E"); 4 }
            0x25 => { let v = self.alu_dec(self.h); self.h = v; log!("  DEC H"); 4 }
            0x2D => { let v = self.alu_dec(self.l); self.l = v; log!("  DEC L"); 4 }
            0x35 => {
                let addr = self.hl();
                let val  = bus.read(addr);
                let v    = self.alu_dec(val);
                bus.write(addr, v);
                log!("  DEC (HL)");
                12
            }
            0x3D => { let v = self.alu_dec(self.a); self.a = v; log!("  DEC A"); 4 }

            // -----------------------------------------------------------------
            // LD r8, d8 — carrega imediato de 8 bits num registrador
            // -----------------------------------------------------------------

            0x06 => { let v = self.fetch_byte(bus); self.b = v; log!("  LD B, {:#04X}", v); 8 }
            0x0E => { let v = self.fetch_byte(bus); self.c = v; log!("  LD C, {:#04X}", v); 8 }
            0x16 => { let v = self.fetch_byte(bus); self.d = v; log!("  LD D, {:#04X}", v); 8 }
            0x1E => { let v = self.fetch_byte(bus); self.e = v; log!("  LD E, {:#04X}", v); 8 }
            0x26 => { let v = self.fetch_byte(bus); self.h = v; log!("  LD H, {:#04X}", v); 8 }
            0x2E => { let v = self.fetch_byte(bus); self.l = v; log!("  LD L, {:#04X}", v); 8 }
            0x36 => {
                let v    = self.fetch_byte(bus);
                let addr = self.hl();
                bus.write(addr, v);
                log!("  LD (HL), {:#04X}", v);
                12
            }
            0x3E => { let v = self.fetch_byte(bus); self.a = v; log!("  LD A, {:#04X}", v); 8 }

            // -----------------------------------------------------------------
            // LD A, (rr)  /  LD (rr), A
            // -----------------------------------------------------------------

            0x02 => { let addr = self.bc(); bus.write(addr, self.a);  log!("  LD (BC), A"); 8 }
            0x12 => { let addr = self.de(); bus.write(addr, self.a);  log!("  LD (DE), A"); 8 }
            0x0A => { let addr = self.bc(); self.a = bus.read(addr);  log!("  LD A, (BC)"); 8 }
            0x1A => { let addr = self.de(); self.a = bus.read(addr);  log!("  LD A, (DE)"); 8 }

            // LD (HL+), A  /  LD (HL-), A  /  LD A, (HL+)  /  LD A, (HL-)
            0x22 => {
                let addr = self.hl();
                bus.write(addr, self.a);
                self.set_hl(addr.wrapping_add(1));
                log!("  LD (HL+), A");
                8
            }
            0x32 => {
                let addr = self.hl();
                bus.write(addr, self.a);
                self.set_hl(addr.wrapping_sub(1));
                log!("  LD (HL-), A");
                8
            }
            0x2A => {
                let addr = self.hl();
                self.a = bus.read(addr);
                self.set_hl(addr.wrapping_add(1));
                log!("  LD A, (HL+)");
                8
            }
            0x3A => {
                let addr = self.hl();
                self.a = bus.read(addr);
                self.set_hl(addr.wrapping_sub(1));
                log!("  LD A, (HL-)");
                8
            }

            // -----------------------------------------------------------------
            // JUMPS RELATIVOS (JR)
            // -----------------------------------------------------------------

            0x18 => {
                let offset = self.fetch_byte(bus) as i8;
                self.pc = self.pc.wrapping_add(offset as u16);
                log!("  JR {:+}", offset);
                12
            }
            0x20 => {
                let offset = self.fetch_byte(bus) as i8;
                if !self.f.zero {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    log!("  JR NZ, {:+}  (saltou)", offset);
                    12
                } else {
                    log!("  JR NZ, {:+}  (não saltou)", offset);
                    8
                }
            }
            0x28 => {
                let offset = self.fetch_byte(bus) as i8;
                if self.f.zero {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    log!("  JR Z, {:+}  (saltou)", offset);
                    12
                } else {
                    log!("  JR Z, {:+}  (não saltou)", offset);
                    8
                }
            }
            0x30 => {
                let offset = self.fetch_byte(bus) as i8;
                if !self.f.carry {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    log!("  JR NC, {:+}  (saltou)", offset);
                    12
                } else {
                    log!("  JR NC, {:+}  (não saltou)", offset);
                    8
                }
            }
            0x38 => {
                let offset = self.fetch_byte(bus) as i8;
                if self.f.carry {
                    self.pc = self.pc.wrapping_add(offset as u16);
                    log!("  JR C, {:+}  (saltou)", offset);
                    12
                } else {
                    log!("  JR C, {:+}  (não saltou)", offset);
                    8
                }
            }

            // -----------------------------------------------------------------
            // BLOCO 0x40–0x7F: LD r8, r8
            // bits 5-3 = destino, bits 2-0 = fonte
            // 0x76 = HALT (tratado acima, não chega aqui)
            // -----------------------------------------------------------------
            0x40..=0x7F => {
                let dst = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let val = self.read_r8(src, bus);
                self.write_r8(dst, val, bus);
                log!("  LD {}, {}", Self::r8_name(dst), Self::r8_name(src));
                if src == 6 || dst == 6 { 8 } else { 4 }
            }

            // -----------------------------------------------------------------
            // BLOCO 0x80–0xBF: ALU A, r8
            // bits 5-3 = operação, bits 2-0 = operando
            // -----------------------------------------------------------------
            0x80..=0xBF => {
                let op  = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let val = self.read_r8(src, bus);
                let cycles = if src == 6 { 8 } else { 4 };
                match op {
                    0 => { self.alu_add(val); log!("  ADD A, {}", Self::r8_name(src)); }
                    1 => { self.alu_adc(val); log!("  ADC A, {}", Self::r8_name(src)); }
                    2 => { self.alu_sub(val); log!("  SUB {}", Self::r8_name(src)); }
                    3 => { self.alu_sbc(val); log!("  SBC A, {}", Self::r8_name(src)); }
                    4 => { self.alu_and(val); log!("  AND {}", Self::r8_name(src)); }
                    5 => { self.alu_xor(val); log!("  XOR {}", Self::r8_name(src)); }
                    6 => { self.alu_or(val);  log!("  OR {}",  Self::r8_name(src)); }
                    7 => { self.alu_cp(val);  log!("  CP {}",  Self::r8_name(src)); }
                    _ => unreachable!(),
                }
                cycles
            }

            // -----------------------------------------------------------------
            // ALU A, d8 — operações com imediato de 8 bits
            // -----------------------------------------------------------------

            0xC6 => { let v = self.fetch_byte(bus); self.alu_add(v); log!("  ADD A, {:#04X}", v); 8 }
            0xCE => { let v = self.fetch_byte(bus); self.alu_adc(v); log!("  ADC A, {:#04X}", v); 8 }
            0xD6 => { let v = self.fetch_byte(bus); self.alu_sub(v); log!("  SUB {:#04X}", v); 8 }
            0xDE => { let v = self.fetch_byte(bus); self.alu_sbc(v); log!("  SBC A, {:#04X}", v); 8 }
            0xE6 => { let v = self.fetch_byte(bus); self.alu_and(v); log!("  AND {:#04X}", v); 8 }
            0xEE => { let v = self.fetch_byte(bus); self.alu_xor(v); log!("  XOR {:#04X}", v); 8 }
            0xF6 => { let v = self.fetch_byte(bus); self.alu_or(v);  log!("  OR {:#04X}",  v); 8 }
            0xFE => { let v = self.fetch_byte(bus); self.alu_cp(v);  log!("  CP {:#04X}",  v); 8 }

            // -----------------------------------------------------------------
            // JP — jumps absolutos
            // -----------------------------------------------------------------

            0xC3 => { let a = self.fetch_word(bus); self.pc = a; log!("  JP {:#06X}", a); 16 }
            0xE9 => { self.pc = self.hl(); log!("  JP (HL) -> {:#06X}", self.pc); 4 }

            0xC2 => {
                let a = self.fetch_word(bus);
                if !self.f.zero  { self.pc = a; log!("  JP NZ, {:#06X}  (saltou)", a); } else { log!("  JP NZ, {:#06X}  (não)", a); }
                16
            }
            0xCA => {
                let a = self.fetch_word(bus);
                if  self.f.zero  { self.pc = a; log!("  JP Z, {:#06X}  (saltou)", a); } else { log!("  JP Z, {:#06X}  (não)", a); }
                16
            }
            0xD2 => {
                let a = self.fetch_word(bus);
                if !self.f.carry { self.pc = a; log!("  JP NC, {:#06X}  (saltou)", a); } else { log!("  JP NC, {:#06X}  (não)", a); }
                16
            }
            0xDA => {
                let a = self.fetch_word(bus);
                if  self.f.carry { self.pc = a; log!("  JP C, {:#06X}  (saltou)", a); } else { log!("  JP C, {:#06X}  (não)", a); }
                16
            }

            // -----------------------------------------------------------------
            // CALL / RET
            // -----------------------------------------------------------------

            0xCD => {
                let addr = self.fetch_word(bus);
                let ret  = self.pc;
                self.stack_push(bus, ret);
                self.pc = addr;
                log!("  CALL {:#06X}", addr);
                24
            }
            0xC4 => {
                let addr = self.fetch_word(bus);
                if !self.f.zero  { let r = self.pc; self.stack_push(bus, r); self.pc = addr; log!("  CALL NZ, {:#06X}  (chamou)", addr); 24 }
                else             { log!("  CALL NZ, {:#06X}  (não)", addr); 12 }
            }
            0xCC => {
                let addr = self.fetch_word(bus);
                if  self.f.zero  { let r = self.pc; self.stack_push(bus, r); self.pc = addr; log!("  CALL Z, {:#06X}  (chamou)", addr); 24 }
                else             { log!("  CALL Z, {:#06X}  (não)", addr); 12 }
            }
            0xD4 => {
                let addr = self.fetch_word(bus);
                if !self.f.carry { let r = self.pc; self.stack_push(bus, r); self.pc = addr; log!("  CALL NC, {:#06X}  (chamou)", addr); 24 }
                else             { log!("  CALL NC, {:#06X}  (não)", addr); 12 }
            }
            0xDC => {
                let addr = self.fetch_word(bus);
                if  self.f.carry { let r = self.pc; self.stack_push(bus, r); self.pc = addr; log!("  CALL C, {:#06X}  (chamou)", addr); 24 }
                else             { log!("  CALL C, {:#06X}  (não)", addr); 12 }
            }

            0xC9 => { self.pc = self.stack_pop(bus); log!("  RET -> {:#06X}", self.pc); 16 }
            0xD9 => { self.pc = self.stack_pop(bus); self.ime = true; log!("  RETI -> {:#06X}", self.pc); 16 }

            0xC0 => {
                if !self.f.zero  { self.pc = self.stack_pop(bus); log!("  RET NZ  (retornou)"); 20 }
                else             { log!("  RET NZ  (não)"); 8 }
            }
            0xC8 => {
                if  self.f.zero  { self.pc = self.stack_pop(bus); log!("  RET Z  (retornou)"); 20 }
                else             { log!("  RET Z  (não)"); 8 }
            }
            0xD0 => {
                if !self.f.carry { self.pc = self.stack_pop(bus); log!("  RET NC  (retornou)"); 20 }
                else             { log!("  RET NC  (não)"); 8 }
            }
            0xD8 => {
                if  self.f.carry { self.pc = self.stack_pop(bus); log!("  RET C  (retornou)"); 20 }
                else             { log!("  RET C  (não)"); 8 }
            }

            // -----------------------------------------------------------------
            // PUSH / POP
            // -----------------------------------------------------------------

            0xC5 => { let v = self.bc(); self.stack_push(bus, v); log!("  PUSH BC"); 16 }
            0xD5 => { let v = self.de(); self.stack_push(bus, v); log!("  PUSH DE"); 16 }
            0xE5 => { let v = self.hl(); self.stack_push(bus, v); log!("  PUSH HL"); 16 }
            0xF5 => { let v = self.af(); self.stack_push(bus, v); log!("  PUSH AF"); 16 }

            0xC1 => { let v = self.stack_pop(bus); self.set_bc(v); log!("  POP BC"); 12 }
            0xD1 => { let v = self.stack_pop(bus); self.set_de(v); log!("  POP DE"); 12 }
            0xE1 => { let v = self.stack_pop(bus); self.set_hl(v); log!("  POP HL"); 12 }
            0xF1 => { let v = self.stack_pop(bus); self.set_af(v); log!("  POP AF"); 12 }

            // -----------------------------------------------------------------
            // RST — chama endereços fixos na página zero
            // -----------------------------------------------------------------

            0xC7 => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x00; log!("  RST 0x00"); 16 }
            0xCF => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x08; log!("  RST 0x08"); 16 }
            0xD7 => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x10; log!("  RST 0x10"); 16 }
            0xDF => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x18; log!("  RST 0x18"); 16 }
            0xE7 => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x20; log!("  RST 0x20"); 16 }
            0xEF => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x28; log!("  RST 0x28"); 16 }
            0xF7 => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x30; log!("  RST 0x30"); 16 }
            0xFF => { let r = self.pc; self.stack_push(bus, r); self.pc = 0x38; log!("  RST 0x38"); 16 }

            // -----------------------------------------------------------------
            // LOADS HIGH MEMORY / I/O
            // -----------------------------------------------------------------

            // LDH (a8), A — escreve A em 0xFF00 + a8
            0xE0 => {
                let offset = self.fetch_byte(bus) as u16;
                bus.write(0xFF00 + offset, self.a);
                log!("  LDH ({:#04X}), A  -> [0xFF{:02X}] = {:02X}", offset, offset, self.a);
                12
            }
            // LDH A, (a8) — lê de 0xFF00 + a8 para A
            0xF0 => {
                let offset = self.fetch_byte(bus) as u16;
                self.a = bus.read(0xFF00 + offset);
                log!("  LDH A, ({:#04X})  -> A = {:02X}", offset, self.a);
                12
            }
            // LD (C), A — escreve A em 0xFF00 + C
            0xE2 => {
                let addr = 0xFF00 + (self.c as u16);
                bus.write(addr, self.a);
                log!("  LD (C), A  -> [0xFF{:02X}] = {:02X}", self.c, self.a);
                8
            }
            // LD A, (C) — lê de 0xFF00 + C para A
            0xF2 => {
                let addr = 0xFF00 + (self.c as u16);
                self.a = bus.read(addr);
                log!("  LD A, (C)  -> A = {:02X}", self.a);
                8
            }
            // LD (nn), A — escreve A em endereço absoluto de 16 bits
            0xEA => {
                let addr = self.fetch_word(bus);
                bus.write(addr, self.a);
                log!("  LD ({:#06X}), A", addr);
                16
            }
            // LD A, (nn) — lê de endereço absoluto de 16 bits para A
            0xFA => {
                let addr = self.fetch_word(bus);
                self.a = bus.read(addr);
                log!("  LD A, ({:#06X})  -> A = {:02X}", addr, self.a);
                16
            }

            // -----------------------------------------------------------------
            // PREFIXO CB — instruções de bit (shift, rotate, test)
            // Cada instrução CB tem o sub-opcode no byte seguinte.
            // Implementaremos este bloco em seguida.
            // -----------------------------------------------------------------
            0xCB => {
                let sub_op = self.fetch_byte(bus);
                log!("  PREFIX CB {:#04X}  (não implementado ainda)", sub_op);
                8
            }

            // -----------------------------------------------------------------
            // OPCODE NÃO IMPLEMENTADO
            // -----------------------------------------------------------------
            _ => {
                log!("  !! {:#04X} NÃO IMPLEMENTADO — travando em {:#06X}", opcode, pc_before);
                self.pc = pc_before;
                4
            }
        }
    }
}
